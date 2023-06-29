use jvm_rs::jni::{jclass, jint, jlong, jobjectArray, JNIEnv, JavaVM, JNI_FALSE, JNI_OK};
use jvm_rs::jvmti::{
    jvmtiCapabilities, jvmtiEnv, jvmtiHeapObjectFilter_JVMTI_HEAP_OBJECT_EITHER,
    jvmtiIterationControl, jvmtiIterationControl_JVMTI_ITERATION_ABORT,
    jvmtiIterationControl_JVMTI_ITERATION_CONTINUE, JVMTI_VERSION_1_2,
};
use once_cell::sync::{Lazy, OnceCell};
use std::ffi::{c_char, c_int, c_uchar, c_void};
use std::ops::Add;

static mut TAG_COUNTER: Lazy<jlong> = Lazy::new(|| 0);

struct LimitCounter {
    current_counter: jint,
    limit_value: jint,
}

impl LimitCounter {
    fn init(&mut self, limit: jint) {
        self.current_counter = 0;
        self.limit_value = limit;
    }

    fn count_down(&mut self) {
        self.current_counter += 1;
    }

    fn allow(&self) -> bool {
        if self.limit_value < 0 {
            true
        } else {
            self.limit_value > self.current_counter
        }
    }
}

static mut JVMTI: OnceCell<jvmtiEnv> = OnceCell::new();

// Init is required before each IterateOverInstancesOfClass call
static mut LIMIT_COUNTER: Lazy<LimitCounter> = Lazy::new(|| LimitCounter {
    current_counter: 0,
    limit_value: 0,
});

extern "C" fn init_agent(vm: *mut JavaVM, _reserved: *mut c_void) -> c_int {
    let result = unsafe {
        JVMTI.get_or_try_init(|| {
            if let Some(f) = (**vm).GetEnv {
                //Get JVMTI environment
                let mut jvmti = std::ptr::null_mut();
                let rc = f(vm, &mut jvmti, JVMTI_VERSION_1_2 as jint);
                if rc != JNI_OK as i32 {
                    let msg =
                        format!("ERROR: Unable to create jvmtiEnv, GetEnv failed, error={rc}");
                    eprintln!("{}", msg);
                    return Err(std::io::Error::new(std::io::ErrorKind::Other, msg));
                }
                let mut capabilities: jvmtiCapabilities = std::mem::zeroed();
                capabilities.set_can_tag_objects(1);
                let mut jvmti: jvmtiEnv = jvmti as jvmtiEnv;
                if let Some(f) = (*jvmti).AddCapabilities {
                    let err = f(&mut jvmti, &capabilities);
                    if err != 0 {
                        let msg = format!("ERROR: JVMTI AddCapabilities failed, error={err}");
                        eprintln!("{}", msg);
                        return Err(std::io::Error::new(std::io::ErrorKind::Other, msg));
                    }
                    return Ok(jvmti);
                }
            }
            let msg = "ERROR: Unable to create jvmtiEnv";
            eprintln!("{}", msg);
            Err(std::io::Error::new(std::io::ErrorKind::Other, msg))
        })
    };
    let r = match result {
        Ok(_) => JNI_OK,
        Err(_) => JNI_FALSE,
    };
    r as c_int
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn Agent_OnLoad(
    vm: *mut JavaVM,
    _options: *mut c_char,
    reserved: *mut c_void,
) -> jint {
    init_agent(vm, reserved)
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn Agent_OnAttach(
    vm: *mut JavaVM,
    _options: *mut c_char,
    reserved: *mut c_void,
) -> jint {
    init_agent(vm, reserved)
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn JNI_OnLoad(vm: *mut JavaVM, reserved: *mut c_void) -> jint {
    init_agent(vm, reserved)
}

#[no_mangle]
#[allow(non_snake_case, clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn Java_org_dromara_dynamictp_jvmti_JVMTI_getInstances0(
    env: *mut JNIEnv,
    _this_class: jclass,
    klass: jclass,
    limit: jint,
) -> jobjectArray {
    let tag = get_tag();
    unsafe {
        LIMIT_COUNTER.init(limit);
        if let Some(jvmti) = JVMTI.get_mut() {
            if let Some(f) = (**jvmti).IterateOverInstancesOfClass {
                let mut err = f(
                    jvmti,
                    klass,
                    jvmtiHeapObjectFilter_JVMTI_HEAP_OBJECT_EITHER,
                    Some(heap_object_callback),
                    &tag as *const _ as *const c_void,
                );
                if err != 0 {
                    eprintln!("ERROR: JVMTI IterateOverInstancesOfClass failed, error={err}");
                    return std::ptr::null_mut();
                }
                if let Some(f) = (**jvmti).GetObjectsWithTags {
                    let mut count = 0;
                    let mut instances = std::ptr::null_mut();
                    err = f(
                        jvmti,
                        1,
                        &tag,
                        &mut count,
                        &mut instances,
                        std::ptr::null_mut(),
                    );
                    if err != 0 {
                        eprintln!("ERROR: JVMTI GetObjectsWithTags failed, error={err}");
                        return std::ptr::null_mut();
                    }
                    if let Some(f) = (**env).NewObjectArray {
                        let array = f(env, count, klass, std::ptr::null_mut());
                        let mut vec =
                            Vec::from_raw_parts(instances, count as usize, count as usize);
                        //add element to array
                        for i in 0..count {
                            if let Some(v) = vec.pop() {
                                if let Some(f) = (**env).SetObjectArrayElement {
                                    f(env, array, i, v);
                                }
                            }
                        }
                        if let Some(f) = (**jvmti).Deallocate {
                            f(jvmti, instances as *mut c_uchar);
                        }
                        return array;
                    }
                }
            }
        }
    }
    std::ptr::null_mut()
}

extern "C" fn get_tag() -> jlong {
    unsafe { TAG_COUNTER.add(1) }
}

unsafe extern "C" fn heap_object_callback(
    _class_tag: jlong,
    _size: jlong,
    tag_ptr: *mut jlong,
    user_data: *mut c_void,
) -> jvmtiIterationControl {
    *tag_ptr = *(user_data as *mut jlong);
    LIMIT_COUNTER.count_down();
    if LIMIT_COUNTER.allow() {
        jvmtiIterationControl_JVMTI_ITERATION_CONTINUE
    } else {
        jvmtiIterationControl_JVMTI_ITERATION_ABORT
    }
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn Java_org_dromara_dynamictp_jvmti_JVMTI_forceGc(
    _env: *mut JNIEnv,
    _this_class: jclass,
) {
    unsafe {
        if let Some(jvmti) = JVMTI.get_mut() {
            if let Some(f) = (**jvmti).ForceGarbageCollection {
                f(jvmti);
            }
        }
    }
}

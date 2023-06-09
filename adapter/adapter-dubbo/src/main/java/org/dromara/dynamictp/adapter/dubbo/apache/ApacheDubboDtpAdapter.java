/*
 * Licensed to the Apache Software Foundation (ASF) under one or more
 * contributor license agreements.  See the NOTICE file distributed with
 * this work for additional information regarding copyright ownership.
 * The ASF licenses this file to You under the Apache License, Version 2.0
 * (the "License"); you may not use this file except in compliance with
 * the License.  You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

package org.dromara.dynamictp.adapter.dubbo.apache;

import org.dromara.dynamictp.adapter.common.AbstractDtpAdapter;
import org.dromara.dynamictp.core.support.ExecutorWrapper;
import org.dromara.dynamictp.common.properties.DtpProperties;
import org.dromara.dynamictp.common.util.ReflectionUtil;
import lombok.extern.slf4j.Slf4j;
import lombok.val;
import org.apache.commons.collections4.MapUtils;
import org.apache.dubbo.common.Version;
import org.apache.dubbo.common.extension.ExtensionLoader;
import org.apache.dubbo.common.store.DataStore;
import org.apache.dubbo.common.threadpool.manager.DefaultExecutorRepository;
import org.apache.dubbo.common.threadpool.manager.ExecutorRepository;
import org.apache.dubbo.rpc.model.ApplicationModel;

import java.util.Map;
import java.util.Objects;
import java.util.concurrent.ConcurrentMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.ThreadPoolExecutor;

/**
 * ApacheDubboDtpAdapter related
 *
 * @author yanhom
 * @since 1.0.6
 */
@Slf4j
@SuppressWarnings("all")
public class ApacheDubboDtpAdapter extends AbstractDtpAdapter {

    private static final String NAME = "dubboTp";

    private static final String EXECUTOR_SERVICE_COMPONENT_KEY = ExecutorService.class.getName();
    
    @Override
    public void refresh(DtpProperties dtpProperties) {
        refresh(NAME, dtpProperties.getDubboTp(), dtpProperties.getPlatforms());
    }

    @Override
    protected void initialize() {
        super.initialize();
        String currVersion = Version.getVersion();
        if (DubboVersion.compare(DubboVersion.VERSION_2_7_5, currVersion) > 0) {
            // 当前dubbo版本 < 2.7.5
            DataStore dataStore = ExtensionLoader.getExtensionLoader(DataStore.class).getDefaultExtension();
            if (Objects.isNull(dataStore)) {
                return;
            }
            Map<String, Object> executorMap = dataStore.get(EXECUTOR_SERVICE_COMPONENT_KEY);
            if (MapUtils.isNotEmpty(executorMap)) {
                executorMap.forEach((k, v) -> doInit(k, (ThreadPoolExecutor) v));
            }
            log.info("DynamicTp adapter, apache dubbo provider executors init end, executors: {}", executors);
            return;
        }

        ExecutorRepository executorRepository;
        if (DubboVersion.compare(currVersion, DubboVersion.VERSION_3_0_3) >= 0) {
            // 当前dubbo版本 >= 3.0.3
            executorRepository = ApplicationModel.defaultModel().getExtensionLoader(ExecutorRepository.class).getDefaultExtension();
        } else {
            // 2.7.5 <= 当前dubbo版本 < 3.0.3
            executorRepository = ExtensionLoader.getExtensionLoader(ExecutorRepository.class).getDefaultExtension();
        }

        val data = (ConcurrentMap<String, ConcurrentMap<Integer, ExecutorService>>) ReflectionUtil.getFieldValue(
                DefaultExecutorRepository.class, "data", executorRepository);
        if (Objects.isNull(data)) {
            return;
        }

        Map<Integer, ExecutorService> executorMap = data.get(EXECUTOR_SERVICE_COMPONENT_KEY);
        if (MapUtils.isNotEmpty(executorMap)) {
            executorMap.forEach((k, v) -> doInit(k.toString(), (ThreadPoolExecutor) v));
        }
        log.info("DynamicTp adapter, apache dubbo provider executors init end, executors: {}", executors);
    }

    private void doInit(String port, ThreadPoolExecutor executor) {
        val name = genTpName(port);
        val executorWrapper = new ExecutorWrapper(name, executor);
        initNotifyItems(name, executorWrapper);
        executors.put(name, executorWrapper);
    }

    private String genTpName(String port) {
        return NAME + "#" + port;
    }
}

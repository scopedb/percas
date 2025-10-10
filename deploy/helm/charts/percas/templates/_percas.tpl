# Copyright 2025 ScopeDB <contact@scopedb.io>
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

{{/*
Percas command
*/}}
{{- define "percas.command" -}}
- /bin/percas
{{- end }}

{{/*
Percas args
*/}}
{{- define "percas.args" -}}
- start
- --config-file
- {{ include "percas.configFile" . }}
{{- end }}

{{/*
Percas config mount path
*/}}
{{- define "percas.configMountPath" -}}
/etc/percas
{{- end }}

{{/*
Percas config file
*/}}
{{- define "percas.configFile" -}}
{{ include "percas.configMountPath" . }}/config.toml
{{- end }}

{{/*
Percas dir
*/}}
{{- define "percas.dir" -}}
/percas
{{- end }}

{{/*
Percas data dir
*/}}
{{- define "percas.dataDir" -}}
{{ printf "%s/data" (include "percas.dir" .) }}
{{- end }}

{{/*
Percas volumes
*/}}
{{- define "percas.volumes" -}}
- name: config
  configMap:
    name: {{ include "percas.fullname" . }}
{{- if not .Values.persistence.enabled}}
- name: data
  emptyDir: {}
{{- end }}
{{- range .Values.statefulSet.volumes }}
- {{- toYaml . }}
{{- end }}
{{- end }}

{{/*
Percas volume mounts
*/}}
{{- define "percas.volumeMounts" -}}
{{- if .Values.statefulSet.volumeMounts }}
{{ toYaml .Values.statefulSet.volumeMounts }}
{{- end }}
- name: data
  mountPath: {{ include "percas.dir" . }}
- name: config
  mountPath: {{ include "percas.configMountPath" . }}
  readOnly: true
{{- end }}

{{/*
Percas volume claim template
*/}}
{{- define "percas.volumeClaimTemplate" -}}
{{- if .Values.statefulSet.volumeClaimTemplates }}
{{ toYaml .Values.statefulSet.volumeClaimTemplates }}
{{- end }}
{{- if .Values.persistence.enabled }}
- metadata:
    name: data
    labels: {{ include "percas.persistenceVolumeLabels" . | nindent 6 }}
    annotations: {{ include "percas.persistenceVolumeAnnotations" . | nindent 6 }}
  spec:
    accessModes: {{ toYaml .Values.persistence.accessModes | nindent 6 }}
    storageClassName: {{ .Values.persistence.storageClass }}
    resources:
      requests:
        storage: {{ .Values.persistence.size }}
{{- end }}
{{- end }}

{{/*
Percas disk capacity
*/}}
{{- define "percas.diskCapacity" -}}
{{ subf (trimSuffix "Gi"  .Values.persistence.size |  mulf 1073741824) 10000 | floor | int }}
{{- end }}

{{/*
Percas headless service domain
*/}}
{{- define "percas.headlessServiceDomain" -}}
{{ printf "%s.%s.svc.%s" (include "percas.headlessService.name" .) .Release.Namespace .Values.clusterDomain }}
{{- end }}

{{/*
Percas cluster ID
*/}}
{{- define "percas.clusterID" -}}
{{ .Values.percas.clusterID | default (printf "%s.%s" .Release.Name .Release.Namespace) }}
{{- end }}

{{/*
Percas environment variables
*/}}
{{- define "percas.env" -}}
{{- if .Values.image.debug }}
- name: PERCAS_DEBUG
  value: "true"
{{- end }}
- name: POD_INDEX
  valueFrom:
    fieldRef:
      fieldPath: metadata.labels['apps.kubernetes.io/pod-index']
- name: PERCAS_CONFIG_SERVER_MODE
  value: "cluster"
- name: PERCAS_CONFIG_SERVER_LISTEN_DATA_ADDR
  value: "0.0.0.0:{{ .Values.service.port }}"
- name: PERCAS_CONFIG_SERVER_ADVERTISE_DATA_ADDR
  value: "{{ include "percas.headlessServiceDomain" . }}:{{ .Values.service.port }}"
- name: PERCAS_CONFIG_SERVER_LISTEN_CTRL_ADDR
  value: "0.0.0.0:{{ .Values.service.ctrlPort }}"
- name: PERCAS_CONFIG_SERVER_ADVERTISE_CTRL_ADDR
  value: "{{ include "percas.fullname" . }}-$(POD_INDEX).{{ include "percas.headlessServiceDomain" . }}:{{ .Values.service.ctrlPort }}"
- name: PERCAS_CONFIG_SERVER_CLUSTER_ID
  value: "{{ include "percas.clusterID" . }}"
{{- end }}

{{/*
Percas environment variables from
*/}}
{{- define "percas.envFrom" -}}
{{- end }}

{{/*
Percas config
*/}}
{{- define "percas.config" -}}
# ---- Begin of auto-generated config ---- #

# server parents are kept empty intentionally
# options will be set with env variables
[server]
dir = "{{ include "percas.dir" . }}"
initial_peers = [
  "{{ include "percas.fullname" . }}-0.{{ include "percas.headlessServiceDomain" . }}:{{ .Values.service.ctrlPort }}"
]

[storage]
data_dir = "{{ include "percas.dataDir" . }}"
disk_capacity = {{ include "percas.diskCapacity" . }}

# ---- End of auto-generated config ---- #
{{- if .Values.percas.config }}

# ---- Begin of user-defined config ---- #

{{ toToml .Values.percas.config }}

# ---- End of user-defined config ---- #
{{- end }}
{{- end }}

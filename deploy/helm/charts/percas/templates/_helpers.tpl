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
Expand the name of the chart.
*/}}
{{- define "percas.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
We truncate at 63 chars because some Kubernetes name fields are limited to this (by the DNS naming spec).
If release name contains chart name it will be used as a full name.
*/}}
{{- define "percas.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "percas.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "percas.labels" -}}
helm.sh/chart: {{ include "percas.chart" . }}
{{ include "percas.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- if .Values.commonLabels }}
{{ toYaml .Values.commonLabels }}
{{- end }}
{{- end }}

{{/*
Selector labels
*/}}
{{- define "percas.selectorLabels" -}}
app.kubernetes.io/name: {{ include "percas.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Common annotations
*/}}
{{- define "percas.annotations" -}}
{{- if .Values.commonAnnotations }}
{{ toYaml .Values.commonAnnotations }}
{{- end }}
{{- end }}

{{/*
Create the name of the service account to use
*/}}
{{- define "percas.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "percas.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Service account labels
*/}}
{{- define "percas.serviceAccountLabels" -}}
{{ include "percas.labels" . }}
{{- if .Values.serviceAccount.labels }}
{{ toYaml .Values.serviceAccount.labels }}
{{- end }}
{{- end }}

{{/*
Service account annotations
*/}}
{{- define "percas.serviceAccountAnnotations" -}}
{{ include "percas.annotations" . }}
{{- if .Values.serviceAccount.annotations }}
{{ toYaml .Values.serviceAccount.annotations }}
{{- end }}
{{- end }}


{{/*
StatefulSet labels
*/}}
{{- define "percas.statefulSetLabels" -}}
{{ include "percas.labels" . }}
{{- if .Values.statefulSet.labels }}
{{ toYaml .Values.statefulSet.labels }}
{{- end }}
{{- end }}

{{/*
StatefulSet annotations
*/}}
{{- define "percas.statefulSetAnnotations" -}}
{{ include "percas.annotations" . }}
{{- if .Values.statefulSet.annotations }}
{{ toYaml .Values.statefulSet.annotations }}
{{- end }}
{{- end }}

{{/*
StatefulSet pod labels
*/}}
{{- define "percas.podLabels" -}}
{{ include "percas.labels" . }}
{{- if .Values.statefulSet.podLabels }}
{{ toYaml .Values.statefulSet.podLabels }}
{{- end }}
{{- end }}

{{/*
StatefulSet pod annotations
*/}}
{{- define "percas.podAnnotations" -}}
{{ include "percas.annotations" . }}
{{- if .Values.statefulSet.podAnnotations }}
{{ toYaml .Values.statefulSet.podAnnotations }}
{{- end }}
{{- end }}

{{/*
Persistence volume labels
*/}}
{{- define "percas.persistenceVolumeLabels" -}}
{{ include "percas.labels" . }}
{{- if .Values.persistence.labels }}
{{ toYaml .Values.persistence.labels }}
{{- end }}
{{- end }}

{{/*
Persistence volume annotations
*/}}
{{- define "percas.persistenceVolumeAnnotations" -}}
{{ include "percas.annotations" . }}
{{- if .Values.persistence.annotations }}
{{ toYaml .Values.persistence.annotations }}
{{- end }}
{{- end }}

{{/*
Service labels
*/}}
{{- define "percas.serviceLabels" -}}
{{ include "percas.labels" . }}
{{- if .Values.service.labels }}
{{ toYaml .Values.service.labels }}
{{- end }}
{{- end }}

{{/*
Service annotations
*/}}
{{- define "percas.serviceAnnotations" -}}
{{ include "percas.annotations" . }}
{{- if .Values.service.annotations }}
{{ toYaml .Values.service.annotations }}
{{- end }}
{{- end }}

{{/*
Headless service name
*/}}
{{- define "percas.headlessService.name" -}}
{{- printf "%s-headless" (include "percas.fullname" .) | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Network policy labels
*/}}
{{- define "percas.networkPolicyLabels" -}}
{{ include "percas.labels" . }}
{{- if .Values.networkPolicy.labels }}
{{ toYaml .Values.networkPolicy.labels }}
{{- end }}
{{- end }}

{{/*
Network policy annotations
*/}}
{{- define "percas.networkPolicyAnnotations" -}}
{{ include "percas.annotations" . }}
{{- if .Values.networkPolicy.annotations }}
{{ toYaml .Values.networkPolicy.annotations }}
{{- end }}
{{- end }}

{{/*
ConfigMap labels
*/}}
{{- define "percas.configMapLabels" -}}
{{ include "percas.labels" . }}
{{- end }}

{{/*
ConfigMap annotations
*/}}
{{- define "percas.configMapAnnotations" -}}
{{ include "percas.annotations" . }}
{{- end }}

{{/*
Percas image
*/}}
{{- define "percas.image" -}}
{{- if eq .Values.image.registry "docker.io" }}
{{- printf "%s:%s" .Values.image.repository (.Values.image.tag | default .Chart.AppVersion) }}
{{- else }}
{{- printf "%s/%s:%s" .Values.image.registry .Values.image.repository (.Values.image.tag | default .Chart.AppVersion) }}
{{- end }}
{{- end }}

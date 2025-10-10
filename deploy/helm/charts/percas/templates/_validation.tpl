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
Percas validation
*/}}
{{- define "percas.validation" -}}
{{- $messages := list -}}
{{- $messages := append $messages (include "percas.validate.serviceAccount" . ) -}}
{{- $messages := append $messages (include "percas.validate.service" . ) -}}

{{- $messages := without $messages "" -}}
{{- $messages := compact $messages -}}
{{- $message := (regexReplaceAll "\n\n+" (join "\n" $messages) "\n" | trim) -}}
{{- if $message -}}
{{- printf "\nVALUES VALIDATION:\n%s" $message | fail }}
{{- end -}}
{{- end }}

{{/*
Validate that the service account is configured correctly.
*/}}
{{- define "percas.validate.serviceAccount" -}}
{{- if not .Values.serviceAccount.create }}
{{- if not .Values.serviceAccount.name }}
- serviceAccount.name must be set when serviceAccount.create is false
{{- end }}
{{- end }}
{{- end }}

{{/*
Validate that the service is configured correctly.
*/}}
{{- define "percas.validate.service" -}}
{{- with .Values.service }}
{{- if eq .port .ctrlPort }}
- service.port and service.ctrlPort must be different
{{- end }}
{{- end }}
{{- end }}

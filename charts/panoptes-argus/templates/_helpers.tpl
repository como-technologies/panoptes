{{/*
Expand the name of the chart.
*/}}
{{- define "panoptes-argus.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "panoptes-argus.fullname" -}}
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
{{- define "panoptes-argus.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels (no name — daemon and controller diverge)
*/}}
{{- define "panoptes-argus.labels" -}}
helm.sh/chart: {{ include "panoptes-argus.chart" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: panoptes
{{- if .Values.global.cluster.name }}
panoptes.io/cluster: {{ .Values.global.cluster.name }}
{{- end }}
{{- if .Values.global.cluster.environment }}
panoptes.io/environment: {{ .Values.global.cluster.environment }}
{{- end }}
{{- end }}

{{/*
Selector labels (instance only — used by ServiceMonitor)
*/}}
{{- define "panoptes-argus.selectorLabels" -}}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Controller selector labels
*/}}
{{- define "panoptes-argus.controller.selectorLabels" -}}
app.kubernetes.io/name: argus-operator
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Controller labels
*/}}
{{- define "panoptes-argus.controller.labels" -}}
{{ include "panoptes-argus.labels" . }}
app.kubernetes.io/name: argus-operator
app.kubernetes.io/component: controller
{{- end }}

{{/*
Daemon selector labels
*/}}
{{- define "panoptes-argus.daemon.selectorLabels" -}}
app.kubernetes.io/name: argusd
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Daemon labels
*/}}
{{- define "panoptes-argus.daemon.labels" -}}
{{ include "panoptes-argus.labels" . }}
app.kubernetes.io/name: argusd
app.kubernetes.io/component: daemon
{{- end }}

{{/*
Service account name
*/}}
{{- define "panoptes-argus.serviceAccountName" -}}
{{- if .Values.controller.serviceAccount.create }}
{{- default (include "panoptes-argus.fullname" .) .Values.controller.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.controller.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Expand the name of the chart.
*/}}
{{- define "panoptes.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "panoptes.fullname" -}}
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
{{- define "panoptes.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "panoptes.labels" -}}
helm.sh/chart: {{ include "panoptes.chart" . }}
{{ include "panoptes.selectorLabels" . }}
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
Selector labels
*/}}
{{- define "panoptes.selectorLabels" -}}
app.kubernetes.io/name: {{ include "panoptes.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Argus controller labels
*/}}
{{- define "panoptes.argus.controller.labels" -}}
{{ include "panoptes.labels" . }}
app.kubernetes.io/component: argus-controller
{{- end }}

{{/*
Argus daemon labels
*/}}
{{- define "panoptes.argus.daemon.labels" -}}
{{ include "panoptes.labels" . }}
app.kubernetes.io/component: argusd
{{- end }}

{{/*
Janus controller labels
*/}}
{{- define "panoptes.janus.controller.labels" -}}
{{ include "panoptes.labels" . }}
app.kubernetes.io/component: janus-controller
{{- end }}

{{/*
Janus daemon labels
*/}}
{{- define "panoptes.janus.daemon.labels" -}}
{{ include "panoptes.labels" . }}
app.kubernetes.io/component: janusd
{{- end }}

{{/*
Dashboard labels
*/}}
{{- define "panoptes.dashboard.labels" -}}
{{ include "panoptes.labels" . }}
app.kubernetes.io/component: dashboard
{{- end }}

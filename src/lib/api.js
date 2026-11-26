// Single gateway to the Rust backend. Components never call `invoke` directly, so the whole
// command surface is visible here and can be mocked in tests.
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export function inTauri() {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export async function call(cmd, args) {
  if (!inTauri()) throw { code: 'not_in_tauri', message: `Not running inside the desktop app (${cmd})` };
  return invoke(cmd, args);
}

export function on(event, handler) {
  if (!inTauri()) return Promise.resolve(() => {});
  return listen(event, (e) => handler(e.payload));
}

export const api = {
  appInfo: () => call('app_info'),
  settingsGet: () => call('settings_get'),
  settingsSave: (settings) => call('settings_save', { settings }),
  dbStatus: () => call('db_status'),
  dbBackup: (dest) => call('db_backup', { dest }),
  dbRestore: (src) => call('db_restore', { src }),
  profileList: () => call('profile_list'),
  profileSave: (profile) => call('profile_save', { profile }),
  profileDelete: (id) => call('profile_delete', { id }),
  historyList: (query = {}) => call('history_list', { query }),
  historyClear: () => call('history_clear'),
  updateCheck: () => call('update_check'),
  updateInstall: () => call('update_install'),

  projectOpen: (path) => call('project_open', { path }),
  projectList: () => call('project_list'),
  projectGet: (id) => call('project_get', { id }),
  projectUpdate: (id, update) => call('project_update', { id, update }),
  projectDelete: (id) => call('project_delete', { id }),
  analysisStart: (projectId, profileId) => call('analysis_start', { projectId, profileId: profileId || null }),
  analysisCancel: (jobId) => call('analysis_cancel', { jobId }),
  explorerList: (projectId, analysisId, path, showHidden, showIgnored) =>
    call('explorer_list', { projectId, analysisId: analysisId || null, path, showHidden, showIgnored }),

  overview: (id) => call('analysis_overview', { id }),
  files: (id, query) => call('analysis_files', { id, query }),
  findings: (id, query = {}) => call('analysis_findings', { id, query }),
  tree: (id, path) => call('analysis_tree', { id, path }),
  fileDetail: (id, path) => call('file_detail', { id, path }),
  graph: (id, query) => call('graph_view', { id, query }),
  dependencies: (id) => call('dependency_view', { id }),

  snapshotList: (projectId) => call('snapshot_list', { projectId }),
  snapshotCreate: (analysisId, name, note) => call('snapshot_create', { analysisId, name, note }),
  snapshotUpdate: (id, name, note) => call('snapshot_update', { id, name, note }),
  snapshotDelete: (id) => call('snapshot_delete', { id }),
  snapshotOpen: (id) => call('snapshot_open', { id }),
  snapshotExport: (id, dest) => call('snapshot_export', { id, dest }),
  snapshotImport: (projectId, src) => call('snapshot_import', { projectId, src }),
  compare: (before, after) => call('compare_analyses', { before, after }),

  mappingGet: (projectId) => call('mapping_get', { projectId }),
  annotationSet: (projectId, annotation) => call('annotation_set', { projectId, annotation }),
  groupSave: (projectId, group) => call('group_save', { projectId, group }),
  groupDelete: (projectId, id) => call('group_delete', { projectId, id }),
  layersSet: (projectId, layers) => call('layers_set', { projectId, layers }),
  relationAdd: (projectId, relation) => call('relation_add', { projectId, relation }),
  relationDelete: (projectId, id) => call('relation_delete', { projectId, id }),
  noteSave: (projectId, note) => call('note_save', { projectId, note }),
  noteDelete: (projectId, id) => call('note_delete', { projectId, id }),
  mappingExport: (projectId, dest) => call('mapping_export', { projectId, dest }),
  mappingImport: (projectId, src) => call('mapping_import', { projectId, src }),

  reportPreview: (request) => call('report_preview', { request }),
  reportExport: (request, dest) => call('report_export', { request, dest }),
  fileExport: (dest, content, base64) => call('file_export', { dest, content, base64: !!base64 }),
};

/** Human-readable message for an error coming from the backend. */
export function errorMessage(e) {
  if (!e) return 'Unknown error';
  if (typeof e === 'string') return e;
  return e.message || e.code || String(e);
}

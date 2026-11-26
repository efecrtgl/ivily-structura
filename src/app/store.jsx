import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react';
import { api, errorMessage, on, inTauri } from '../lib/api.js';
import { useToast } from '../components/ui.jsx';

const Ctx = createContext(null);
export const useApp = () => useContext(Ctx);

const LAST_PROJECT_KEY = 'structura.lastProject';

function applyTheme(theme, reduceMotion) {
  const root = document.documentElement;
  const resolved = theme === 'system' ? (window.matchMedia?.('(prefers-color-scheme: light)').matches ? 'light' : 'dark') : theme;
  root.dataset.theme = resolved;
  root.dataset.motion = reduceMotion ? 'reduced' : 'full';
}

export function AppProvider({ children }) {
  const toast = useToast();
  const [info, setInfo] = useState(null);
  const [settings, setSettings] = useState(null);
  const [project, setProject] = useState(null);
  const [analysis, setAnalysis] = useState(null);
  const [job, setJob] = useState(null);
  const [route, setRoute] = useState({ page: 'welcome', params: null });
  const [bootError, setBootError] = useState(null);
  const projectRef = useRef(null);
  projectRef.current = project;

  const navigate = useCallback((page, params = null) => setRoute({ page, params }), []);

  // Boot: info + settings (+ reopen last project).
  useEffect(() => {
    if (!inTauri()) {
      applyTheme('dark', false);
      setBootError({ message: 'Ivily Structura must run as a desktop app (npm run tauri dev).' });
      return;
    }
    Promise.all([api.appInfo(), api.settingsGet()])
      .then(async ([i, s]) => {
        setInfo(i);
        setSettings(s);
        applyTheme(s.theme, s.reduceMotion);
        if (i.startupPath) {
          try {
            const r = await api.projectOpen(i.startupPath);
            setProject(r.project);
            setAnalysis(r.analysis);
            localStorage.setItem(LAST_PROJECT_KEY, String(r.project.id));
            setRoute({ page: 'dashboard', params: null });
            if (i.startupAnalyze && r.project.exists) {
              const jobId = await api.analysisStart(r.project.id, null);
              setJob({ jobId, projectId: r.project.id, phase: 'scan', done: 0, total: 0 });
            }
            return;
          } catch {
            /* fall through to the normal start */
          }
        }
        const last = Number(localStorage.getItem(LAST_PROJECT_KEY));
        if (s.reopenLastProject && last) {
          try {
            const r = await api.projectGet(last);
            if (r.project.exists) {
              setProject(r.project);
              setAnalysis(r.analysis);
              setRoute({ page: 'dashboard', params: null });
            }
          } catch {
            localStorage.removeItem(LAST_PROJECT_KEY);
          }
        }
      })
      .catch(setBootError);
  }, []);

  // Follow the OS theme when set to "system".
  useEffect(() => {
    if (!settings) return undefined;
    applyTheme(settings.theme, settings.reduceMotion);
    if (settings.theme !== 'system' || !window.matchMedia) return undefined;
    const mq = window.matchMedia('(prefers-color-scheme: light)');
    const f = () => applyTheme('system', settings.reduceMotion);
    mq.addEventListener('change', f);
    return () => mq.removeEventListener('change', f);
  }, [settings]);

  // Analysis events.
  useEffect(() => {
    const offs = [
      on('analysis-progress', (p) => setJob((j) => (j && j.jobId === p.jobId ? { ...j, ...p } : j))),
      on('analysis-done', (d) => {
        setJob((j) => (j && j.jobId === d.jobId ? null : j));
        if (d.cancelled) {
          toast('Analysis cancelled');
        } else if (d.error) {
          toast(`Analysis failed: ${errorMessage(d.error)}`, 'error');
        } else if (d.analysis && projectRef.current?.id === d.projectId) {
          setAnalysis(d.analysis);
          api.projectGet(d.projectId).then((r) => setProject(r.project)).catch(() => {});
          toast(`Analysis complete — ${d.analysis.summary.files.toLocaleString('en-US')} files in ${d.analysis.meta.durationMs} ms`);
        }
      }),
    ];
    return () => offs.forEach((p) => p.then((off) => off()));
  }, [toast]);

  const openPath = useCallback(
    async (path) => {
      const r = await api.projectOpen(path);
      setProject(r.project);
      setAnalysis(r.analysis);
      localStorage.setItem(LAST_PROJECT_KEY, String(r.project.id));
      setRoute({ page: 'dashboard', params: null });
      return r;
    },
    [],
  );

  // ivily-structura.exe <folder> while already running.
  useEffect(() => {
    const off = on('open-path', (p) => openPath(p).catch((e) => toast(errorMessage(e), 'error')));
    return () => {
      off.then((f) => f());
    };
  }, [openPath, toast]);

  const openProject = useCallback(async (id) => {
    const r = await api.projectGet(id);
    setProject(r.project);
    setAnalysis(r.analysis);
    localStorage.setItem(LAST_PROJECT_KEY, String(id));
    setRoute({ page: 'dashboard', params: null });
  }, []);

  const closeProject = useCallback(() => {
    setProject(null);
    setAnalysis(null);
    localStorage.removeItem(LAST_PROJECT_KEY);
    setRoute({ page: 'welcome', params: null });
  }, []);

  const analyze = useCallback(
    async (profileId) => {
      if (!project) return;
      try {
        const jobId = await api.analysisStart(project.id, profileId);
        setJob({ jobId, projectId: project.id, phase: 'scan', done: 0, total: 0, profileId });
      } catch (e) {
        toast(errorMessage(e), 'error');
      }
    },
    [project, toast],
  );

  const cancel = useCallback(() => {
    if (job) api.analysisCancel(job.jobId).catch(() => {});
  }, [job]);

  const saveSettings = useCallback(async (s) => {
    const saved = await api.settingsSave(s);
    setSettings(saved);
    return saved;
  }, []);

  const value = useMemo(
    () => ({ info, settings, project, setProject, analysis, setAnalysis, job, route, navigate, openPath, openProject, closeProject, analyze, cancel, saveSettings, bootError, toast }),
    [info, settings, project, analysis, job, route, navigate, openPath, openProject, closeProject, analyze, cancel, saveSettings, bootError, toast],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

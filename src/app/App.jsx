import { lazy, Suspense } from 'react';
import { AppProvider, useApp } from './store.jsx';
import { Loading, ToastProvider } from '../components/ui.jsx';
import TitleBar from '../components/TitleBar.jsx';
import Icon from '../components/Icon.jsx';
import ErrorBoundary from '../components/ErrorBoundary.jsx';
import JobBar from '../components/JobBar.jsx';
import Welcome from '../pages/Welcome.jsx';

const PAGES = {
  welcome: Welcome,
  dashboard: lazy(() => import('../pages/Dashboard.jsx')),
  explorer: lazy(() => import('../pages/Explorer.jsx')),
  graph: lazy(() => import('../pages/Graph.jsx')),
  health: lazy(() => import('../pages/Health.jsx')),
  dependencies: lazy(() => import('../pages/Dependencies.jsx')),
  snapshots: lazy(() => import('../pages/Snapshots.jsx')),
  mapping: lazy(() => import('../pages/Mapping.jsx')),
  reports: lazy(() => import('../pages/Reports.jsx')),
  history: lazy(() => import('../pages/History.jsx')),
  settings: lazy(() => import('../pages/Settings.jsx')),
};

const NAV = [
  {
    label: 'Project',
    items: [
      { id: 'dashboard', label: 'Dashboard', icon: 'dashboard', needs: 'analysis' },
      { id: 'explorer', label: 'Explorer', icon: 'explorer', needs: 'project' },
      { id: 'graph', label: 'Architecture graph', icon: 'graph', needs: 'deps' },
      { id: 'health', label: 'Health', icon: 'health', needs: 'analysis' },
      { id: 'dependencies', label: 'Dependencies', icon: 'deps', needs: 'deps' },
    ],
  },
  {
    label: 'Workspace',
    items: [
      { id: 'snapshots', label: 'Snapshots & compare', icon: 'snapshot', needs: 'project' },
      { id: 'mapping', label: 'Project mapping', icon: 'map', needs: 'project' },
      { id: 'reports', label: 'Reports', icon: 'report', needs: 'analysis' },
    ],
  },
  {
    label: 'App',
    items: [
      { id: 'history', label: 'History', icon: 'history' },
      { id: 'settings', label: 'Settings', icon: 'settings' },
    ],
  },
];

function Sidebar() {
  const { project, analysis, route, navigate, closeProject, settings, saveSettings } = useApp();
  const enabled = (needs) => {
    if (!needs) return true;
    if (!project) return false;
    if (needs === 'analysis') return !!analysis;
    if (needs === 'deps') return !!analysis?.meta?.dependenciesAnalyzed;
    return true;
  };
  const theme = settings?.theme || 'system';
  const nextTheme = theme === 'dark' ? 'light' : theme === 'light' ? 'system' : 'dark';
  return (
    <nav className="sidebar" aria-label="Main navigation">
      {project ? (
        <button type="button" className="project-switch" onClick={closeProject} title="Close project and choose another">
          <Icon name="folder" />
          <div style={{ minWidth: 0 }}>
            <div className="pname">{project.name}</div>
            <div className="proot">{project.root}</div>
          </div>
        </button>
      ) : (
        <button type="button" className="project-switch" onClick={() => navigate('welcome')}>
          <Icon name="plus" />
          <div className="pname">Open a project</div>
        </button>
      )}
      {NAV.map((g) => (
        <div key={g.label}>
          <div className="nav-label">{g.label}</div>
          {g.items.map((i) => (
            <button
              key={i.id}
              type="button"
              className="nav-item"
              aria-current={route.page === i.id ? 'page' : undefined}
              disabled={!enabled(i.needs)}
              title={!enabled(i.needs) ? (i.needs === 'deps' ? 'Requires an analysis with dependency analysis' : i.needs === 'analysis' ? 'Analyse the project first' : 'Open a project first') : undefined}
              onClick={() => navigate(i.id)}
            >
              <Icon name={i.icon} />
              <span>{i.label}</span>
            </button>
          ))}
        </div>
      ))}
      <div className="sidebar-footer">
        <span>v{__APP_VERSION__}</span>
        {settings && (
          <button type="button" className="btn ghost sm icon" title={`Theme: ${theme}`} aria-label={`Theme: ${theme}. Switch to ${nextTheme}`} onClick={() => saveSettings({ ...settings, theme: nextTheme })}>
            <Icon name={theme === 'light' ? 'sun' : theme === 'dark' ? 'moon' : 'monitor'} size={14} />
          </button>
        )}
      </div>
    </nav>
  );
}

function Shell() {
  const { route, project, analysis, bootError } = useApp();
  const Page = PAGES[route.page] || Welcome;
  const crumb = project ? (
    <>
      <b>{project.name}</b>
      {analysis && ` · ${analysis.source === 'snapshot' ? 'snapshot view' : `${analysis.meta.profileName}`}`}
    </>
  ) : null;
  return (
    <div className="app">
      <TitleBar crumb={crumb} />
      <div className="shell">
        <Sidebar />
        <main className="main">
          <JobBar />
          {bootError ? (
            <div className="page">
              <div className="banner danger">{bootError.message || String(bootError)}</div>
            </div>
          ) : (
            <ErrorBoundary key={route.page}>
              <Suspense fallback={<Loading />}>
                <Page params={route.params} />
              </Suspense>
            </ErrorBoundary>
          )}
        </main>
      </div>
    </div>
  );
}

export default function App() {
  return (
    <ToastProvider>
      <AppProvider>
        <Shell />
      </AppProvider>
    </ToastProvider>
  );
}

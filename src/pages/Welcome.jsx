import { useApp } from '../app/store.jsx';
import { api, errorMessage } from '../lib/api.js';
import { useAsync } from '../lib/hooks.js';
import { pickFolder, confirm } from '../lib/dialogs.js';
import { relative, num } from '../lib/format.js';
import Icon from '../components/Icon.jsx';
import logo from '../../assets/logo.svg';

export default function Welcome() {
  const { openPath, openProject, toast, info } = useApp();
  const projects = useAsync(() => (info ? api.projectList() : Promise.resolve([])), [info]);

  const open = async () => {
    const p = await pickFolder();
    if (!p) return;
    try {
      await openPath(p);
    } catch (e) {
      toast(errorMessage(e), 'error');
    }
  };
  const forget = async (e, p) => {
    e.stopPropagation();
    if (!(await confirm(`Forget "${p.name}"? Its snapshots, mapping and history links are removed from Structura. The project folder itself is not touched.`))) return;
    await api.projectDelete(p.id).catch((x) => toast(errorMessage(x), 'error'));
    projects.reload();
  };
  const pin = async (e, p) => {
    e.stopPropagation();
    await api.projectUpdate(p.id, { pinned: !p.pinned });
    projects.reload();
  };

  return (
    <div className="welcome">
      <div className="welcome-inner">
        <div className="hero">
          <img src={logo} alt="" />
          <div>
            <h1>Understand any codebase.</h1>
            <p>Structure, dependencies and architecture health — analysed locally, explained honestly.</p>
          </div>
        </div>
        <div className="row">
          <button type="button" className="btn primary" style={{ height: 38, padding: '0 18px' }} onClick={open}>
            <Icon name="folder" /> Open project folder
          </button>
          <span className="faint small">Structura only reads your files. Nothing is uploaded.</span>
        </div>

        <div className="card flush">
          <div className="card-head" style={{ padding: '14px 18px 0' }}>
            <h3>Recent projects</h3>
            <span className="hint">{projects.data?.length || 0} known</span>
          </div>
          <div className="list" style={{ padding: '4px 10px 10px' }}>
            {projects.data?.length === 0 && <div className="faint small" style={{ padding: 12 }}>No projects yet. Open a folder to start.</div>}
            {projects.data?.map((p) => (
              <div key={p.id} className="li clickable" onClick={() => p.exists && openProject(p.id).catch((e) => toast(errorMessage(e), 'error'))} role="button" tabIndex={0} style={{ opacity: p.exists ? 1 : 0.5 }}>
                <Icon name={p.pinned ? 'pin' : 'folder'} />
                <div style={{ minWidth: 0, flex: 1 }}>
                  <div className="ellipsis" style={{ fontWeight: 500 }}>
                    {p.name}
                    {!p.exists && <span className="badge" style={{ marginLeft: 8 }}>folder missing</span>}
                  </div>
                  <div className="path ellipsis">{p.root}</div>
                </div>
                <div className="faint xs" style={{ textAlign: 'right' }}>
                  {p.lastSummary ? `${num(p.lastSummary.files)} files` : 'not analysed'}
                  <div>{relative(p.lastOpenedAt || p.createdAt)}</div>
                </div>
                <button type="button" className="btn ghost sm icon" title={p.pinned ? 'Unpin' : 'Pin'} onClick={(e) => pin(e, p)}>
                  <Icon name="pin" size={14} />
                </button>
                <button type="button" className="btn ghost sm icon" title="Forget project" onClick={(e) => forget(e, p)}>
                  <Icon name="trash" size={14} />
                </button>
              </div>
            ))}
          </div>
        </div>

        <div className="principles">
          <div className="card">
            <b>Read-only</b>Projects are never modified. All data lives in Structura's local database.
          </div>
          <div className="card">
            <b>Honest analysis</b>Every finding shows its confidence and evidence. No opaque scores.
          </div>
          <div className="card">
            <b>Local-first</b>No telemetry, no uploads. Works offline; updates are optional.
          </div>
        </div>
      </div>
    </div>
  );
}

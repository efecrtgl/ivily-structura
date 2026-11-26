import { useApp } from '../app/store.jsx';
import { api } from '../lib/api.js';
import { useAsync } from '../lib/hooks.js';
import { bytes, compact, date, duration, num, relative, ROLE_LABEL, SEVERITY_ORDER } from '../lib/format.js';
import { Async, BarList, Donut, Empty, SeverityBadge, Stat } from '../components/ui.jsx';
import AnalyzeControl from '../components/AnalyzeControl.jsx';
import Icon from '../components/Icon.jsx';

export default function Dashboard() {
  const { project, analysis, navigate } = useApp();
  const overview = useAsync(() => (analysis ? api.overview(analysis.id) : Promise.resolve(null)), [analysis?.id]);
  const snapshots = useAsync(() => (project ? api.snapshotList(project.id) : Promise.resolve([])), [project?.id, analysis?.id]);
  const history = useAsync(() => (project ? api.historyList({ projectId: project.id, limit: 6 }) : Promise.resolve([])), [project?.id, analysis?.id]);
  const mapping = useAsync(() => (project ? api.mappingGet(project.id) : Promise.resolve(null)), [project?.id]);

  if (!project) return null;
  if (!analysis) {
    return (
      <div className="page">
        <div className="page-header">
          <div>
            <h1>{project.name}</h1>
            <div className="sub path">{project.root}</div>
          </div>
        </div>
        <div className="card">
          <Empty icon="play" title="Not analysed yet" action={<AnalyzeControl />}>
            Choose a profile and start the analysis. <b>Quick scan</b> reads only file metadata; <b>Full architecture analysis</b> also reads source files to build the dependency graph.
          </Empty>
        </div>
      </div>
    );
  }

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <h1>{project.name}</h1>
          <div className="sub">
            {analysis.source === 'snapshot' ? (
              <span className="badge solid" style={{ marginRight: 8 }}>
                Snapshot view
              </span>
            ) : null}
            Analysed {relative(analysis.meta.startedAt)} with <b>{analysis.meta.profileName}</b> · {duration(analysis.meta.durationMs)}
          </div>
        </div>
        <div className="actions">
          <AnalyzeControl />
        </div>
      </div>
      {!project.exists && <div className="banner warn" style={{ marginBottom: 16 }}>The project folder no longer exists. You are viewing stored results.</div>}
      <Async state={overview}>{(o) => o && <Overview o={o} navigate={navigate} snapshots={snapshots.data} history={history.data} mapping={mapping.data} />}</Async>
    </div>
  );
}

function Overview({ o, navigate, snapshots, history, mapping }) {
  const m = o.metrics;
  const sev = o.findingsBySeverity; // [info, low, medium, high]
  const sevCount = { info: sev[0], low: sev[1], medium: sev[2], high: sev[3] };
  const langItems = o.languages.slice(0, 7).map((l) => ({ label: l.name, value: l.files }));
  const rest = o.languages.slice(7).reduce((s, l) => s + l.files, 0);
  if (rest) langItems.push({ label: 'Other', value: rest });
  const frameworks = [...new Set(o.ecosystems.flatMap((e) => e.frameworks.map((f) => f.name)))];
  const important = mapping?.annotations?.filter((a) => a.important || a.label) || [];

  return (
    <div className="stack" style={{ gap: 16 }}>
      {o.meta.truncated && <div className="banner warn"><Icon name="alert" /> The scan stopped at the profile's entry limit — results cover part of the project.</div>}
      <div className="grid g4">
        <Stat label="Files" value={compact(m.totalFiles)} note={`${num(m.totalDirs)} directories · ${bytes(m.totalBytes)}`} onClick={() => navigate('explorer')} />
        <Stat
          label="Source code"
          value={o.meta.contentAnalyzed ? compact(m.sourceCodeLines) : '—'}
          note={o.meta.contentAnalyzed ? 'estimated code lines in source files' : 'not measured by this profile'}
          onClick={() => navigate('explorer', { sort: 'lines' })}
        />
        <Stat
          label="Dependencies"
          value={o.meta.dependenciesAnalyzed ? compact(m.internalEdges) : '—'}
          note={o.meta.dependenciesAnalyzed ? `internal edges · ${num(m.externalPackages)} external packages` : 'not analysed by this profile'}
          onClick={o.meta.dependenciesAnalyzed ? () => navigate('dependencies') : undefined}
        />
        <Stat
          label="Circular dependencies"
          value={o.meta.dependenciesAnalyzed ? num(m.cycles) : '—'}
          note={m.cycles ? `${num(m.filesInCycles)} files involved` : 'none detected'}
          onClick={o.meta.dependenciesAnalyzed ? () => navigate('graph', { onlyCycles: m.cycles > 0 }) : undefined}
        />
      </div>

      <div className="grid g3">
        <div className="card">
          <div className="card-head">
            <h3>Languages</h3>
            <span className="hint">by file count</span>
          </div>
          {langItems.length ? <Donut items={langItems} onClick={(i) => i.label !== 'Other' && navigate('explorer', { language: o.languages.find((l) => l.name === i.label)?.id })} /> : <span className="faint small">No recognised languages.</span>}
        </div>

        <div className="card">
          <div className="card-head">
            <h3>Architecture health</h3>
            <button type="button" className="btn ghost sm" onClick={() => navigate('health')}>
              All findings <Icon name="chevron" size={12} />
            </button>
          </div>
          {!o.meta.healthAnalyzed ? (
            <span className="faint small">Health rules were not run.</span>
          ) : (
            <>
              <div className="row" style={{ gap: 16, marginBottom: 12 }}>
                {SEVERITY_ORDER.map((s) => (
                  <button key={s} type="button" className="btn ghost" style={{ flexDirection: 'column', height: 'auto', padding: '4px 8px', alignItems: 'flex-start' }} onClick={() => navigate('health', { severity: s })}>
                    <span className="stat">
                      <span className="value" style={{ fontSize: 22 }}>{sevCount[s]}</span>
                    </span>
                    <SeverityBadge severity={s} />
                  </button>
                ))}
              </div>
              <div className="list">
                {o.topFindings.slice(0, 4).map((f, i) => (
                  <div key={i} className="li clickable" onClick={() => navigate('health', { rule: f.rule })}>
                    <span className={`dot ${f.severity}`} />
                    <span className="ellipsis" title={f.title}>{f.title}</span>
                  </div>
                ))}
                {o.topFindings.length === 0 && <span className="faint small">No findings for the enabled rules.</span>}
              </div>
            </>
          )}
        </div>

        <div className="card">
          <div className="card-head">
            <h3>Project</h3>
          </div>
          <dl className="kv">
            <dt>Ecosystems</dt>
            <dd>{o.ecosystems.length ? [...new Set(o.ecosystems.map((e) => e.name))].join(', ') : '—'}</dd>
            <dt>Frameworks</dt>
            <dd>{frameworks.length ? frameworks.join(', ') : '—'}</dd>
            <dt>Patterns</dt>
            <dd>{o.structure.patterns.length ? o.structure.patterns.map((p) => p.name).join(', ') : '—'}</dd>
            <dt>Entry points</dt>
            <dd>
              {o.structure.entryPoints.slice(0, 3).map((e) => (
                <div key={e.path} className="path ellipsis" title={e.reason}>{e.path}</div>
              ))}
              {o.structure.entryPoints.length === 0 && '—'}
            </dd>
            <dt>README</dt>
            <dd>{o.documentation.readme ? <span className="path">{o.documentation.readme}</span> : <span className="badge medium">missing</span>}</dd>
          </dl>
        </div>
      </div>

      <div className="grid g3">
        <div className="card">
          <div className="card-head">
            <h3>Directory overview</h3>
            <span className="hint">top-level, by size</span>
          </div>
          <BarList items={o.topDirs.map((d) => ({ label: d.path, value: d.totalBytes, key: d.path, title: `${num(d.totalFiles)} files` }))} format={bytes} onClick={(i) => navigate('explorer', { path: i.key })} />
          {o.topDirs.length === 0 && <span className="faint small">All files are at the project root.</span>}
        </div>
        <div className="card">
          <div className="card-head">
            <h3>Largest files</h3>
            <span className="hint">{o.meta.contentAnalyzed ? 'by lines' : 'by size'}</span>
          </div>
          <BarList
            items={(o.meta.contentAnalyzed ? m.longestFiles : m.largestFiles).slice(0, 7).map((f) => ({ label: f.path, key: f.path, value: o.meta.contentAnalyzed ? f.lines : f.bytes }))}
            format={o.meta.contentAnalyzed ? (v) => `${num(v)} lines` : bytes}
            onClick={(i) => navigate('explorer', { file: i.key })}
          />
        </div>
        <div className="card">
          <div className="card-head">
            <h3>Most depended-upon</h3>
            <span className="hint">fan-in</span>
          </div>
          {o.hubs.length ? (
            <BarList items={o.hubs.slice(0, 7).map((h) => ({ label: h.path, key: h.path, value: h.fanIn }))} format={(v) => `${v} files`} onClick={(i) => navigate('graph', { focus: i.key })} />
          ) : (
            <span className="faint small">{o.meta.dependenciesAnalyzed ? 'No internal imports found.' : 'Dependency analysis was not run.'}</span>
          )}
        </div>
      </div>

      <div className="grid g3">
        <div className="card">
          <div className="card-head">
            <h3>Complexity indicators</h3>
            <span className="hint">estimated</span>
          </div>
          <dl className="kv">
            <dt>Avg. decision points / file</dt>
            <dd className="num">{o.meta.contentAnalyzed ? m.avgComplexity.toFixed(1) : '—'}</dd>
            <dt>Max decision points</dt>
            <dd className="num">{o.meta.contentAnalyzed ? num(m.maxComplexity) : '—'}</dd>
            <dt>Max fan-in / fan-out</dt>
            <dd className="num">{o.meta.dependenciesAnalyzed ? `${m.maxFanIn} / ${m.maxFanOut}` : '—'}</dd>
            <dt>Max directory depth</dt>
            <dd className="num">{m.maxDepth}</dd>
            <dt>Files by role</dt>
            <dd>
              {Object.entries(m.byRole)
                .sort((a, b) => b[1] - a[1])
                .slice(0, 5)
                .map(([r, c]) => `${ROLE_LABEL[r] || r} ${num(c)}`)
                .join(' · ')}
            </dd>
          </dl>
        </div>
        <div className="card">
          <div className="card-head">
            <h3>Snapshots</h3>
            <button type="button" className="btn ghost sm" onClick={() => navigate('snapshots')}>
              Manage <Icon name="chevron" size={12} />
            </button>
          </div>
          <div className="list">
            {(snapshots || []).slice(0, 4).map((s) => (
              <div key={s.id} className="li clickable" onClick={() => navigate('snapshots', { select: s.id })}>
                <Icon name="snapshot" />
                <span className="ellipsis" style={{ flex: 1 }}>{s.name}</span>
                <span className="faint xs">{relative(s.createdAt)}</span>
              </div>
            ))}
            {!snapshots?.length && <span className="faint small">No snapshots yet. Save one to track how the structure evolves.</span>}
          </div>
        </div>
        <div className="card">
          <div className="card-head">
            <h3>Recent analyses</h3>
            <button type="button" className="btn ghost sm" onClick={() => navigate('history')}>
              History <Icon name="chevron" size={12} />
            </button>
          </div>
          <div className="list">
            {(history || []).map((h) => (
              <div key={h.id} className="li">
                <span className={`dot ${h.outcome === 'success' ? '' : h.outcome === 'failure' ? 'high' : 'medium'}`} />
                <span className="ellipsis" style={{ flex: 1 }}>{h.title}</span>
                <span className="faint xs" title={date(h.ts)}>{relative(h.ts)}</span>
              </div>
            ))}
          </div>
        </div>
      </div>

      {important.length > 0 && (
        <div className="card">
          <div className="card-head">
            <h3>Important modules & notes</h3>
            <button type="button" className="btn ghost sm" onClick={() => navigate('mapping')}>
              Mapping <Icon name="chevron" size={12} />
            </button>
          </div>
          <div className="list">
            {important.slice(0, 8).map((a) => (
              <div key={a.id} className="li clickable" onClick={() => navigate('explorer', { path: a.path })}>
                <Icon name={a.important ? 'star' : 'note'} />
                <span className="path" style={{ minWidth: 160 }}>{a.path || '(root)'}</span>
                <span className="ellipsis">{a.label}{a.note ? ` — ${a.note}` : ''}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {o.warningCount > 0 && (
        <details className="card">
          <summary className="small" style={{ cursor: 'pointer' }}>
            {o.warningCount} scan warnings (permission errors, skipped symlinks, limits)
          </summary>
          <div className="list" style={{ marginTop: 8 }}>
            {o.warnings.map((w, i) => (
              <div key={i} className="li">
                <span className="badge">{w.kind}</span>
                <span className="path">{w.path || '(project)'}</span>
                <span className="faint xs ellipsis">{w.message}</span>
              </div>
            ))}
          </div>
        </details>
      )}
    </div>
  );
}

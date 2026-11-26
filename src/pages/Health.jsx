import { useState } from 'react';
import { useApp } from '../app/store.jsx';
import { api } from '../lib/api.js';
import { useAsync, useDebounced } from '../lib/hooks.js';
import { SEVERITY_ORDER } from '../lib/format.js';
import { Async, ConfidenceBadge, Empty, SearchInput, SeverityBadge, Tabs } from '../components/ui.jsx';
import Icon from '../components/Icon.jsx';

const CATEGORIES = ['dependencies', 'size', 'structure', 'documentation', 'testing', 'configuration', 'security'];

function FindingCard({ f, defaultOpen, navigate }) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="finding">
      <div className="finding-head" onClick={() => setOpen(!open)} role="button" aria-expanded={open} tabIndex={0} onKeyDown={(e) => e.key === 'Enter' && setOpen(!open)}>
        <span className={`dot ${f.severity}`} />
        <span className="title ellipsis" title={f.title}>{f.title}</span>
        <span className="badge">{f.category}</span>
        <SeverityBadge severity={f.severity} />
        <Icon name={open ? 'chevronDown' : 'chevron'} size={14} />
      </div>
      {open && (
        <div className="finding-body">
          <div className="row wrap">
            <ConfidenceBadge confidence={f.confidence} />
            <span className="badge mono">{f.rule}</span>
            {f.metric && (
              <span className="badge" title="Measured value vs. configured threshold">
                {f.metric.name}: {Number.isInteger(f.metric.value) ? f.metric.value : f.metric.value.toFixed(2)} (threshold {f.metric.threshold})
              </span>
            )}
          </div>
          <p className="muted">{f.explanation}</p>
          {f.evidence.length > 0 && (
            <div>
              <div className="section-title">Evidence</div>
              <ul className="evidence selectable">
                {f.evidence.map((e, i) => (
                  <li key={i}>{e}</li>
                ))}
              </ul>
            </div>
          )}
          {f.files.length > 0 && (
            <div>
              <div className="section-title">Affected ({f.files.length})</div>
              <div className="row wrap">
                {f.files.slice(0, 40).map((p) => (
                  <button key={p} type="button" className="chip mono" style={{ cursor: 'pointer' }} onClick={() => navigate('explorer', { file: p, path: p })}>
                    {p}
                  </button>
                ))}
                {f.files.length > 40 && <span className="faint xs">+{f.files.length - 40} more</span>}
              </div>
            </div>
          )}
          <div className="suggestion">
            <b>Suggestion: </b>
            {f.suggestion}
          </div>
        </div>
      )}
    </div>
  );
}

export default function Health({ params }) {
  const { analysis, info, navigate } = useApp();
  const [tab, setTab] = useState('findings');
  const [severities, setSeverities] = useState(params?.severity ? [params.severity] : []);
  const [category, setCategory] = useState('');
  const [search, setSearch] = useState(params?.rule || '');
  const q = useDebounced(search, 200);
  const findings = useAsync(() => api.findings(analysis.id, { severities, categories: category ? [category] : [], search: q || null }), [analysis.id, severities, category, q]);
  const all = useAsync(() => api.findings(analysis.id, {}), [analysis.id]);
  const toggleSev = (s) => setSeverities((v) => (v.includes(s) ? v.filter((x) => x !== s) : [...v, s]));
  const counts = SEVERITY_ORDER.reduce((m, s) => ({ ...m, [s]: (all.data || []).filter((f) => f.severity === s).length }), {});

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <h1>Architecture health</h1>
          <div className="sub">Indicators with evidence and confidence — not verdicts. Structura deliberately computes no aggregate score.</div>
        </div>
      </div>
      {!analysis.meta.healthAnalyzed && <div className="banner warn" style={{ marginBottom: 12 }}>This analysis ran without health rules. Re-analyse with a profile that enables them.</div>}
      {!analysis.meta.dependenciesAnalyzed && analysis.meta.healthAnalyzed && <div className="banner info" style={{ marginBottom: 12 }}><Icon name="info" /> Dependency-based rules (cycles, coupling, unused modules) were skipped because this profile does not build the dependency graph.</div>}
      <Tabs tabs={[{ id: 'findings', label: 'Findings', count: all.data?.length }, { id: 'rules', label: 'Rules' }]} value={tab} onChange={setTab} />
      {tab === 'findings' ? (
        <>
          <div className="toolbar">
            {SEVERITY_ORDER.map((s) => (
              <button key={s} type="button" className={`btn sm ${severities.includes(s) ? 'primary' : ''}`} onClick={() => toggleSev(s)} aria-pressed={severities.includes(s)}>
                <span className={`dot ${s}`} /> {s[0].toUpperCase() + s.slice(1)} <span className="faint">{counts[s]}</span>
              </button>
            ))}
            <select className="select" value={category} onChange={(e) => setCategory(e.target.value)} aria-label="Category">
              <option value="">All categories</option>
              {CATEGORIES.map((c) => (
                <option key={c} value={c}>
                  {c}
                </option>
              ))}
            </select>
            <SearchInput value={search} onChange={setSearch} placeholder="Search findings, rules, files…" style={{ flex: 1, minWidth: 220 }} />
          </div>
          <Async state={findings}>
            {(list) =>
              list.length === 0 ? (
                <div className="card">
                  <Empty icon="check" title={all.data?.length ? 'No findings match the filters' : 'No findings'}>
                    {all.data?.length ? 'Adjust the filters above.' : 'None of the enabled rules found an issue. That does not prove the architecture is perfect — only that these indicators are below their thresholds.'}
                  </Empty>
                </div>
              ) : (
                <div>
                  {list.map((f, i) => (
                    <FindingCard key={`${f.rule}-${i}`} f={f} defaultOpen={i === 0 && !!params?.rule} navigate={navigate} />
                  ))}
                </div>
              )
            }
          </Async>
        </>
      ) : (
        <div className="card flush table-wrap">
          <table className="data">
            <thead>
              <tr>
                <th>Rule</th>
                <th>Category</th>
                <th>What it measures</th>
                <th>Needs dependency graph</th>
              </tr>
            </thead>
            <tbody>
              {info?.rules.map((r) => (
                <tr key={r.id}>
                  <td>
                    <div style={{ fontWeight: 500 }}>{r.title}</div>
                    <div className="mono faint xs">{r.id}</div>
                  </td>
                  <td>{r.category}</td>
                  <td className="muted small">{r.description}</td>
                  <td>{r.requiresDependencies ? 'yes' : '—'}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <div className="faint xs" style={{ padding: 12 }}>Rules, thresholds and categories are configured per analysis profile in Settings → Profiles. Layer rules use the layers defined in Project mapping.</div>
        </div>
      )}
    </div>
  );
}

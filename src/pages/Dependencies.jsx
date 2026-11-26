import { useMemo, useState } from 'react';
import { useApp } from '../app/store.jsx';
import { api } from '../lib/api.js';
import { useAsync } from '../lib/hooks.js';
import { num } from '../lib/format.js';
import { Async, BarList, Empty, SearchInput, Segmented, Tabs } from '../components/ui.jsx';
import Icon from '../components/Icon.jsx';

export default function Dependencies() {
  const { analysis, navigate } = useApp();
  const deps = useAsync(() => api.dependencies(analysis.id), [analysis.id]);
  const overview = useAsync(() => api.overview(analysis.id), [analysis.id]);
  const [tab, setTab] = useState('packages');
  return (
    <div className="page">
      <div className="page-header">
        <div>
          <h1>Dependencies</h1>
          <div className="sub">Internal module relationships, external packages, ecosystems and circular dependencies.</div>
        </div>
      </div>
      <Async state={deps}>
        {(d) => (
          <>
            {overview.data && <Resolution r={overview.data.resolution} />}
            <Tabs
              value={tab}
              onChange={setTab}
              tabs={[
                { id: 'packages', label: 'External packages', count: d.packages.length },
                { id: 'ecosystems', label: 'Ecosystems & frameworks', count: d.ecosystems.length },
                { id: 'cycles', label: 'Circular dependencies', count: d.cycles.length },
                { id: 'unresolved', label: 'Unresolved imports', count: d.unresolvedTotal },
              ]}
            />
            {tab === 'packages' && <Packages packages={d.packages} />}
            {tab === 'ecosystems' && <Ecosystems ecosystems={d.ecosystems} />}
            {tab === 'cycles' && <Cycles cycles={d.cycles} navigate={navigate} />}
            {tab === 'unresolved' && <Unresolved d={d} navigate={navigate} />}
          </>
        )}
      </Async>
    </div>
  );
}

function Resolution({ r }) {
  const items = [
    { label: 'Internal (resolved to a project file)', value: r.internal },
    { label: 'External packages', value: r.external },
    { label: 'Built-in / standard library', value: r.builtin },
    { label: 'Unresolved', value: r.unresolved },
  ];
  return (
    <div className="grid g2" style={{ marginBottom: 16 }}>
      <div className="card">
        <div className="card-head">
          <h3>{num(r.total)} imports analysed</h3>
          <span className="hint">static, lexical extraction</span>
        </div>
        <BarList items={items} max={r.total || 1} format={num} />
      </div>
      <div className="card">
        <div className="card-head">
          <h3>How internal imports were resolved</h3>
          <span className="hint">transparency</span>
        </div>
        <BarList items={Object.entries(r.methods).map(([k, v]) => ({ label: k, value: v }))} format={num} />
        {Object.keys(r.methods).length === 0 && <span className="faint small">No internal imports.</span>}
      </div>
    </div>
  );
}

function Packages({ packages }) {
  const [filter, setFilter] = useState('used');
  const [search, setSearch] = useState('');
  const [eco, setEco] = useState('');
  const ecos = useMemo(() => [...new Set(packages.map((p) => p.ecosystem))], [packages]);
  const rows = packages
    .filter((p) => (filter === 'used' ? p.useCount > 0 : filter === 'undeclared' ? p.useCount > 0 && !p.declared : filter === 'unused' ? p.useCount === 0 : true))
    .filter((p) => !eco || p.ecosystem === eco)
    .filter((p) => !search || p.name.toLowerCase().includes(search.toLowerCase()));
  return (
    <div className="card flush">
      <div className="toolbar" style={{ padding: '12px 12px 0' }}>
        <Segmented label="Filter" value={filter} onChange={setFilter} options={[{ value: 'used', label: 'Used' }, { value: 'undeclared', label: 'Undeclared' }, { value: 'unused', label: 'Declared, not imported' }, { value: 'all', label: 'All' }]} />
        <select className="select" value={eco} onChange={(e) => setEco(e.target.value)} aria-label="Ecosystem">
          <option value="">All ecosystems</option>
          {ecos.map((e) => (
            <option key={e} value={e}>
              {e}
            </option>
          ))}
        </select>
        <SearchInput value={search} onChange={setSearch} placeholder="Search packages…" style={{ flex: 1 }} />
      </div>
      {rows.length === 0 ? (
        <Empty icon="package" title="No packages" />
      ) : (
        <div className="table-wrap" style={{ maxHeight: 'calc(100vh - 380px)' }}>
          <table className="data">
            <thead>
              <tr>
                <th>Package</th>
                <th>Ecosystem</th>
                <th>Declared</th>
                <th>Version</th>
                <th className="r">Imports</th>
                <th className="r">Files</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((p) => (
                <tr key={`${p.ecosystem}:${p.name}`}>
                  <td className="mono">{p.name}</td>
                  <td>{p.ecosystem}</td>
                  <td>{p.declared ? <span className="badge">{p.declaredKind || 'yes'}</span> : <span className="badge medium">no</span>}</td>
                  <td className="mono faint xs">{p.version || '—'}</td>
                  <td className="r num">{num(p.useCount)}</td>
                  <td className="r num">{num(p.users.length)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function Ecosystems({ ecosystems }) {
  if (!ecosystems.length) return <div className="card"><Empty icon="package" title="No package manifests found">Structura detects package.json, Cargo.toml, go.mod, pyproject.toml, composer.json, Gemfile, pom.xml, Gradle, .csproj and more.</Empty></div>;
  return (
    <div className="grid g2">
      {ecosystems.map((e) => (
        <div key={e.manifest} className="card">
          <div className="card-head">
            <h3>{e.name}</h3>
            <span className="path">{e.manifest}</span>
          </div>
          <dl className="kv">
            {e.packageName && (
              <>
                <dt>Package</dt>
                <dd className="mono">{e.packageName}{e.version ? ` @ ${e.version}` : ''}</dd>
              </>
            )}
            <dt>Package manager</dt>
            <dd>{e.packageManager || '—'}</dd>
            <dt>Dependencies</dt>
            <dd>{e.dependencies.length} declared</dd>
            {e.entryPoints.length > 0 && (
              <>
                <dt>Entry points</dt>
                <dd>{e.entryPoints.map((p) => <div key={p} className="path">{p}</div>)}</dd>
              </>
            )}
            {e.workspaceMembers.length > 0 && (
              <>
                <dt>Workspaces</dt>
                <dd className="mono small">{e.workspaceMembers.join(', ')}</dd>
              </>
            )}
          </dl>
          {e.frameworks.length > 0 && (
            <>
              <div className="section-title" style={{ marginTop: 12 }}>Frameworks & tools</div>
              <div className="list">
                {e.frameworks.map((f) => (
                  <div key={f.id} className="li">
                    <b className="small">{f.name}</b>
                    <span className="faint xs ellipsis">{f.evidence}</span>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>
      ))}
    </div>
  );
}

function Cycles({ cycles, navigate }) {
  if (!cycles.length) return <div className="card"><Empty icon="check" title="No circular dependencies">No cycles between statically resolved imports. Dynamic or reflection-based loading is not visible to static analysis.</Empty></div>;
  return (
    <div className="stack">
      {cycles.map((c, i) => (
        <div key={i} className="card">
          <div className="card-head">
            <h3>
              <Icon name="cycle" size={14} /> Cycle of {c.files.length} files
            </h3>
            <button type="button" className="btn sm" onClick={() => navigate('graph', { focus: c.path[0], onlyCycles: true })}>
              <Icon name="graph" size={14} /> Show in graph
            </button>
          </div>
          <div className="row wrap" style={{ gap: 6 }}>
            {[...c.path, c.path[0]].map((p, k) => (
              <span key={k} className="row" style={{ gap: 6 }}>
                {k > 0 && <span className="faint">→</span>}
                <button type="button" className="chip mono" style={{ cursor: 'pointer' }} onClick={() => navigate('explorer', { file: p })}>
                  {p}
                </button>
              </span>
            ))}
          </div>
          {c.files.length > c.path.length && <div className="faint xs" style={{ marginTop: 8 }}>The strongly connected component contains {c.files.length} files; the path above is one concrete loop.</div>}
        </div>
      ))}
    </div>
  );
}

function Unresolved({ d, navigate }) {
  if (!d.unresolved.length) return <div className="card"><Empty icon="check" title="Every import was resolved" /></div>;
  return (
    <div className="card flush">
      <div className="faint xs" style={{ padding: 12 }}>
        Imports that look local but could not be mapped to a file (aliases configured in code, generated files, dynamic paths, excluded folders). They are never guessed.
      </div>
      <div className="table-wrap" style={{ maxHeight: 'calc(100vh - 360px)' }}>
        <table className="data">
          <thead>
            <tr>
              <th>File</th>
              <th>Line</th>
              <th>Specifier</th>
              <th>Kind</th>
            </tr>
          </thead>
          <tbody>
            {d.unresolved.map((u, i) => (
              <tr key={i} className="clickable" onClick={() => navigate('explorer', { file: u.path })}>
                <td className="path">{u.path}</td>
                <td className="num">{u.line}</td>
                <td className="mono">{u.spec}</td>
                <td className="faint">{u.kind}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

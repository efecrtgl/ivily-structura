import { useEffect, useMemo, useRef, useState } from 'react';
import cytoscape from 'cytoscape';
import { useApp } from '../app/store.jsx';
import { api, errorMessage } from '../lib/api.js';
import { useDebounced } from '../lib/hooks.js';
import { num } from '../lib/format.js';
import { pickSave, safeFileName } from '../lib/dialogs.js';
import { Check, Empty, ErrorState, Loading, SearchInput, Segmented } from '../components/ui.jsx';
import FilePanel from '../components/FilePanel.jsx';
import Icon from '../components/Icon.jsx';

const LAYOUTS = [
  { value: 'cose', label: 'Force' },
  { value: 'breadthfirst', label: 'Hierarchy' },
  { value: 'concentric', label: 'Concentric' },
  { value: 'circle', label: 'Circle' },
  { value: 'grid', label: 'Grid' },
];

function cssVar(name) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

function layoutOptions(name, n) {
  const base = { name, animate: n < 160, animationDuration: 350, fit: true, padding: 30 };
  if (name === 'cose') return { ...base, nodeRepulsion: () => 9000, idealEdgeLength: () => 70, gravity: 0.25, numIter: n > 400 ? 600 : 1200, randomize: true };
  if (name === 'breadthfirst') return { ...base, directed: true, spacingFactor: 1.05 };
  if (name === 'concentric') return { ...base, concentric: (node) => node.data('fanIn'), levelWidth: () => 2, minNodeSpacing: 12 };
  return base;
}

export default function Graph({ params }) {
  const { analysis, settings, project, toast } = useApp();
  const fileCount = analysis?.summary?.files || 0;
  const [level, setLevel] = useState(params?.focus ? 'file' : fileCount > 400 ? 'directory' : 'file');
  const [dirDepth, setDirDepth] = useState(2);
  const [layout, setLayout] = useState(settings.graphLayout || 'cose');
  const [maxNodes, setMaxNodes] = useState(settings.graphMaxNodes || 300);
  const [includeExternal, setIncludeExternal] = useState(false);
  const [includeTests, setIncludeTests] = useState(true);
  const [includeContains, setIncludeContains] = useState(false);
  const [onlyCycles, setOnlyCycles] = useState(!!params?.onlyCycles);
  const [focus, setFocus] = useState(params?.focus || null);
  const [focusDepth, setFocusDepth] = useState(1);
  const [direction, setDirection] = useState('both');
  const [pathPrefix, setPathPrefix] = useState('');
  const [search, setSearch] = useState('');
  const [view, setView] = useState(null);
  const [error, setError] = useState(null);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState(null);
  const q = useDebounced(search, 250);
  const prefix = useDebounced(pathPrefix, 300);
  const container = useRef(null);
  const cy = useRef(null);

  const query = useMemo(
    () => ({ level, dirDepth, includeExternal, includeContains, includeTests, focus, focusDepth, direction, search: q || null, onlyCycles, pathPrefix: prefix || null, maxNodes, roles: [], languages: [] }),
    [level, dirDepth, includeExternal, includeContains, includeTests, focus, focusDepth, direction, q, onlyCycles, prefix, maxNodes],
  );

  useEffect(() => {
    let alive = true;
    setLoading(true);
    api
      .graph(analysis.id, query)
      .then((v) => {
        if (!alive) return;
        setView(v);
        setError(null);
      })
      .catch((e) => alive && setError(e))
      .finally(() => alive && setLoading(false));
    return () => {
      alive = false;
    };
  }, [analysis.id, query]);

  // Render with Cytoscape.
  useEffect(() => {
    if (!view || !container.current) return undefined;
    const fg = cssVar('--graph-node') || '#ddd';
    const fg2 = cssVar('--graph-node-2') || '#888';
    const edge = cssVar('--graph-edge') || 'rgba(128,128,128,.3)';
    const danger = cssVar('--danger') || '#f58a81';
    const text = cssVar('--text') || '#eee';
    const bg = cssVar('--surface-solid') || '#111';
    const maxSize = Math.max(1, ...view.nodes.map((n) => n.size || 0));
    const elements = [
      ...view.nodes.map((n) => ({
        data: { ...n, w: 14 + Math.sqrt((n.size || 0) / maxSize) * 30 },
        classes: [n.kind, n.inCycle ? 'cycle' : '', n.entry ? 'entry' : '', n.focus ? 'focus' : '', n.role === 'test' ? 'test' : ''].join(' '),
      })),
      ...view.edges.map((e, i) => ({ data: { id: `e${i}`, source: e.source, target: e.target, weight: e.weight, kind: e.kind }, classes: [e.cyclic ? 'cyclic' : '', e.kind].join(' ') })),
    ];
    const inst = cytoscape({
      container: container.current,
      elements,
      wheelSensitivity: 0.25,
      minZoom: 0.05,
      maxZoom: 4,
      style: [
        { selector: 'node', style: { 'background-color': fg, width: 'data(w)', height: 'data(w)', label: 'data(label)', color: text, 'font-size': 9, 'text-valign': 'bottom', 'text-margin-y': 4, 'text-outline-color': bg, 'text-outline-width': 2, 'min-zoomed-font-size': 7 } },
        { selector: 'node.directory', style: { shape: 'round-rectangle', 'background-color': fg2, 'border-width': 1, 'border-color': fg } },
        { selector: 'node.package', style: { shape: 'diamond', 'background-color': 'transparent', 'border-width': 1.5, 'border-color': fg2, color: fg2 } },
        { selector: 'node.test', style: { 'background-opacity': 0.45 } },
        { selector: 'node.entry', style: { 'border-width': 3, 'border-color': text, 'border-style': 'double' } },
        { selector: 'node.cycle', style: { 'background-color': danger } },
        { selector: 'node.focus', style: { 'border-width': 4, 'border-color': text, 'font-weight': 700 } },
        { selector: 'node:selected', style: { 'overlay-color': text, 'overlay-opacity': 0.15, 'overlay-padding': 6 } },
        { selector: 'node.faded', style: { opacity: 0.15 } },
        { selector: 'edge', style: { width: 'mapData(weight, 1, 20, 0.8, 3.5)', 'line-color': edge, 'target-arrow-color': edge, 'target-arrow-shape': 'triangle', 'arrow-scale': 0.7, 'curve-style': 'bezier' } },
        { selector: 'edge.contains', style: { 'line-style': 'dashed', 'target-arrow-shape': 'none' } },
        { selector: 'edge.cyclic', style: { 'line-color': danger, 'target-arrow-color': danger, width: 2 } },
        { selector: 'edge.faded', style: { opacity: 0.06 } },
        { selector: 'edge.hl', style: { 'line-color': text, 'target-arrow-color': text, width: 2 } },
      ],
      layout: layoutOptions(layout, view.nodes.length),
    });
    inst.on('tap', 'node', (evt) => {
      const n = evt.target;
      setSelected(n.data());
      inst.elements().removeClass('faded hl');
      const hood = n.closedNeighborhood();
      inst.elements().not(hood).addClass('faded');
      n.connectedEdges().addClass('hl');
    });
    inst.on('tap', (evt) => {
      if (evt.target === inst) {
        inst.elements().removeClass('faded hl');
        setSelected(null);
      }
    });
    inst.on('dbltap', 'node.directory', (evt) => {
      setPathPrefix(evt.target.data('path'));
      setLevel('file');
    });
    cy.current = inst;
    return () => {
      inst.destroy();
      cy.current = null;
    };
    // Theme changes re-render via the settings dependency.
  }, [view, layout, settings.theme]);

  const exportPng = async () => {
    if (!cy.current) return;
    const dest = await pickSave('Export graph image', `${safeFileName(project.name)}-graph.png`, ['png'], 'PNG image');
    if (!dest) return;
    const data = cy.current.png({ full: true, scale: 2, bg: cssVar('--bg') || '#000' });
    api.fileExport(dest, data, true).then(() => toast('Graph image exported'), (e) => toast(errorMessage(e), 'error'));
  };
  const exportJson = async () => {
    if (!view) return;
    const dest = await pickSave('Export graph data', `${safeFileName(project.name)}-graph.json`, ['json'], 'JSON');
    if (!dest) return;
    const doc = { format: 'ivily-structura-graph', version: 1, query, nodes: view.nodes, edges: view.edges };
    api.fileExport(dest, JSON.stringify(doc, null, 2), false).then(() => toast('Graph data exported'), (e) => toast(errorMessage(e), 'error'));
  };

  const selectedFile = selected?.kind === 'file' ? selected.path : null;

  return (
    <div className="page full">
      <div className="page-header">
        <div>
          <h1>Architecture graph</h1>
          <div className="sub">
            {view ? `${num(view.nodes.length)} nodes · ${num(view.edges.length)} edges` : ' '}
            {view?.truncated && ` (of ${num(view.totalNodes)})`} · arrows point from importer to imported
          </div>
        </div>
        <div className="actions">
          <button type="button" className="btn sm" onClick={exportPng} disabled={!view}>
            <Icon name="image" size={14} /> PNG
          </button>
          <button type="button" className="btn sm" onClick={exportJson} disabled={!view}>
            <Icon name="download" size={14} /> JSON
          </button>
        </div>
      </div>

      <div className="toolbar">
        <Segmented label="Level" value={level} onChange={setLevel} options={[{ value: 'file', label: 'Files' }, { value: 'directory', label: 'Folders' }]} />
        {level === 'directory' && (
          <select className="select" value={dirDepth} onChange={(e) => setDirDepth(Number(e.target.value))} aria-label="Folder depth">
            {[1, 2, 3, 4, 5].map((d) => (
              <option key={d} value={d}>
                Depth {d}
              </option>
            ))}
          </select>
        )}
        <select className="select" value={layout} onChange={(e) => setLayout(e.target.value)} aria-label="Layout">
          {LAYOUTS.map((l) => (
            <option key={l.value} value={l.value}>
              {l.label}
            </option>
          ))}
        </select>
        <SearchInput value={search} onChange={setSearch} placeholder="Highlight path…" style={{ width: 190 }} />
        <input className="input" style={{ width: 160 }} placeholder="Path prefix (e.g. src/ui)" value={pathPrefix} onChange={(e) => setPathPrefix(e.target.value)} aria-label="Path prefix filter" />
        <select className="select" value={maxNodes} onChange={(e) => setMaxNodes(Number(e.target.value))} aria-label="Node limit">
          {[100, 200, 300, 500, 1000, 2000].map((n) => (
            <option key={n} value={n}>
              ≤ {n} nodes
            </option>
          ))}
        </select>
        <Check checked={onlyCycles} onChange={setOnlyCycles}>
          Cycles only
        </Check>
        <Check checked={includeExternal} onChange={setIncludeExternal}>
          External
        </Check>
        <Check checked={includeTests} onChange={setIncludeTests}>
          Tests
        </Check>
        <Check checked={includeContains} onChange={setIncludeContains}>
          Containment
        </Check>
      </div>
      {focus && (
        <div className="toolbar">
          <span className="chip">
            <Icon name="focus" size={12} /> Focus: <span className="mono">{focus}</span>
            <button type="button" className="btn ghost sm icon" aria-label="Clear focus" onClick={() => setFocus(null)}>
              <Icon name="x" size={12} />
            </button>
          </span>
          <Segmented label="Direction" value={direction} onChange={setDirection} options={[{ value: 'both', label: 'Both' }, { value: 'out', label: 'Uses' }, { value: 'in', label: 'Used by' }]} />
          <select className="select" value={focusDepth} onChange={(e) => setFocusDepth(Number(e.target.value))} aria-label="Dependency depth">
            {[1, 2, 3, 4, 6].map((d) => (
              <option key={d} value={d}>
                Depth {d}
              </option>
            ))}
          </select>
        </div>
      )}
      {view?.hint && <div className="banner info small" style={{ marginBottom: 8 }}><Icon name="info" /> {view.hint}</div>}

      <div className="split" style={{ gridTemplateColumns: selected ? 'minmax(0,1fr) minmax(320px, 380px)' : 'minmax(0,1fr)' }}>
        <div className="graph-stage">
          {error && <div style={{ padding: 16 }}><ErrorState error={error} /></div>}
          {!error && view && view.nodes.length === 0 && (
            <Empty icon="graph" title="Nothing to show">
              No internal dependencies match the current filters.
            </Empty>
          )}
          <div className="cy" ref={container} aria-label="Dependency graph" />
          {loading && (
            <div style={{ position: 'absolute', top: 12, left: 12 }}>
              <Loading label="Building graph…" />
            </div>
          )}
          <div className="graph-overlay">
            <span><span className="legend-swatch" style={{ background: 'var(--graph-node)' }} />file</span>
            <span><span className="legend-swatch" style={{ background: 'var(--graph-node-2)', borderRadius: 2 }} />folder</span>
            <span><span className="legend-swatch" style={{ background: 'var(--danger)' }} />cycle</span>
            <span><span className="legend-swatch" style={{ border: '2px double var(--text)' }} />entry</span>
            {includeExternal && <span><span className="legend-swatch" style={{ border: '1px solid var(--graph-node-2)', borderRadius: 0, transform: 'rotate(45deg)' }} />package</span>}
          </div>
          <div className="graph-zoom">
            <button type="button" className="btn icon sm" aria-label="Zoom in" onClick={() => cy.current?.zoom({ level: cy.current.zoom() * 1.25, renderedPosition: { x: container.current.clientWidth / 2, y: container.current.clientHeight / 2 } })}>
              <Icon name="plus" size={14} />
            </button>
            <button type="button" className="btn icon sm" aria-label="Zoom out" onClick={() => cy.current?.zoom({ level: cy.current.zoom() / 1.25, renderedPosition: { x: container.current.clientWidth / 2, y: container.current.clientHeight / 2 } })}>
              <Icon name="minimize" size={14} />
            </button>
            <button type="button" className="btn icon sm" aria-label="Fit" onClick={() => cy.current?.fit(undefined, 30)}>
              <Icon name="maximize" size={12} />
            </button>
          </div>
        </div>
        {selected &&
          (selectedFile ? (
            <FilePanel path={selectedFile} onSelect={(p) => setFocus(p)} onClose={() => setSelected(null)} />
          ) : (
            <aside className="side-panel">
              <div className="ph">
                <Icon name={selected.kind === 'package' ? 'package' : 'folder'} />
                <span style={{ fontWeight: 600, flex: 1 }} className="ellipsis">{selected.label}</span>
                <button type="button" className="btn ghost sm icon" aria-label="Close" onClick={() => setSelected(null)}>
                  <Icon name="x" size={14} />
                </button>
              </div>
              <div className="pb">
                <div className="path">{selected.path}</div>
                <dl className="kv">
                  {selected.kind === 'directory' && (
                    <>
                      <dt>Files in graph</dt>
                      <dd>{selected.files}</dd>
                      <dt>Lines</dt>
                      <dd>{num(selected.size)}</dd>
                    </>
                  )}
                  <dt>{selected.kind === 'package' ? 'Used by' : 'Incoming'}</dt>
                  <dd>{selected.fanIn}</dd>
                  {selected.kind === 'directory' && (
                    <>
                      <dt>Outgoing</dt>
                      <dd>{selected.fanOut}</dd>
                    </>
                  )}
                </dl>
                {selected.kind === 'directory' && (
                  <button type="button" className="btn" onClick={() => { setPathPrefix(selected.path); setLevel('file'); setSelected(null); }}>
                    <Icon name="focus" size={14} /> Expand into files
                  </button>
                )}
              </div>
            </aside>
          ))}
      </div>
      {selectedFile && (
        <div className="row" style={{ marginTop: 8 }}>
          <button type="button" className="btn sm" onClick={() => setFocus(selectedFile)}>
            <Icon name="focus" size={14} /> Focus on {selectedFile.split('/').pop()}
          </button>
        </div>
      )}
    </div>
  );
}

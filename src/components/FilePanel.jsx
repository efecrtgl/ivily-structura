import { useEffect, useState } from 'react';
import { useApp } from '../app/store.jsx';
import { api, errorMessage } from '../lib/api.js';
import { useAsync } from '../lib/hooks.js';
import { bytes, date, num, relative, ROLE_LABEL } from '../lib/format.js';
import { Async, Check, SeverityBadge } from './ui.jsx';
import Icon from './Icon.jsx';

const KIND_ICON = { function: 'code', method: 'code', class: 'layers', struct: 'layers', interface: 'link', trait: 'link', enum: 'layers', type: 'link', constant: 'lock', variable: 'note', module: 'package', namespace: 'package', macro: 'code', resource: 'database', table: 'database' };

/** File intelligence panel. */
export default function FilePanel({ path, onSelect, onClose }) {
  const { analysis, project, navigate, toast } = useApp();
  const detail = useAsync(() => api.fileDetail(analysis.id, path), [analysis?.id, path]);
  const [tab, setTab] = useState('deps');
  return (
    <aside className="side-panel" aria-label="File details">
      <div className="ph">
        <Icon name="file" />
        <span className="ellipsis" style={{ fontWeight: 600, flex: 1 }} title={path}>
          {path.split('/').pop()}
        </span>
        {analysis.meta.dependenciesAnalyzed && (
          <button type="button" className="btn ghost sm" title="Show in graph" onClick={() => navigate('graph', { focus: path })}>
            <Icon name="graph" size={14} />
          </button>
        )}
        {onClose && (
          <button type="button" className="btn ghost sm icon" aria-label="Close" onClick={onClose}>
            <Icon name="x" size={14} />
          </button>
        )}
      </div>
      <div className="pb">
        <Async state={detail}>
          {(d) => (
            <>
              <div className="path selectable">{path}</div>
              <div className="row wrap">
                <span className="badge">{ROLE_LABEL[d.file.role] || d.file.role}</span>
                {d.languageName && <span className="badge" title={`Analysis level: ${d.support}`}>{d.languageName} · {d.support}</span>}
                {d.inCycle && <span className="badge high">in a cycle</span>}
                {d.file.sensitive && <span className="badge medium">sensitive · not read</span>}
                {d.file.hidden && <span className="badge">hidden</span>}
              </div>
              <dl className="kv">
                <dt>Size</dt>
                <dd>{bytes(d.file.size)}</dd>
                {d.file.lines && (
                  <>
                    <dt>Lines</dt>
                    <dd className="num">
                      {num(d.file.lines.total)} total · {num(d.file.lines.code)} code<span className="faint"> (est.)</span> · {num(d.file.lines.comment)} comment<span className="faint"> (est.)</span>
                    </dd>
                  </>
                )}
                {d.file.complexity != null && (
                  <>
                    <dt>Complexity</dt>
                    <dd className="num">
                      {d.file.complexity} decision points · nesting {d.file.maxNesting}
                      <span className="faint"> (estimated)</span>
                    </dd>
                  </>
                )}
                {analysis.meta.dependenciesAnalyzed && (
                  <>
                    <dt>Coupling</dt>
                    <dd className="num">
                      used by {d.file.fanIn} · uses {d.file.fanOut} · {d.file.externalDeps} external
                    </dd>
                  </>
                )}
                <dt>Modified</dt>
                <dd>{date(d.file.modified)}</dd>
                {d.file.namespace && (
                  <>
                    <dt>Namespace</dt>
                    <dd className="mono">{d.file.namespace}</dd>
                  </>
                )}
                {d.file.skipped && (
                  <>
                    <dt>Content</dt>
                    <dd>not analysed ({d.file.skipped})</dd>
                  </>
                )}
              </dl>

              <div className="seg" role="tablist">
                {[
                  ['deps', `Imports ${d.imports.length}`],
                  ['refs', `Used by ${d.referencedBy.length}`],
                  ['symbols', `Symbols ${d.symbols.length}`],
                  ['history', 'History'],
                  ['notes', 'Notes'],
                ].map(([id, label]) => (
                  <button key={id} type="button" aria-pressed={tab === id} onClick={() => setTab(id)}>
                    {label}
                  </button>
                ))}
              </div>

              {tab === 'deps' && (
                <div className="list">
                  {d.imports.length === 0 && <span className="faint small">{d.support === 'detected' || !d.support ? 'Import extraction is not available for this file type.' : 'No imports.'}</span>}
                  {d.imports.map((i, k) => (
                    <div key={k} className={`li ${i.targets.length ? 'clickable' : ''}`} onClick={() => i.targets[0] && onSelect?.(i.targets[0])} title={i.via ? `resolved via ${i.via}` : undefined}>
                      <span className="faint xs num" style={{ width: 30 }}>:{i.line}</span>
                      <div style={{ minWidth: 0, flex: 1 }}>
                        <div className="mono ellipsis small">{i.spec}</div>
                        {i.targets.map((t) => (
                          <div key={t} className="path ellipsis">→ {t}</div>
                        ))}
                      </div>
                      <span className={`badge ${i.targetKind === 'unresolved' ? 'medium' : ''}`}>{i.targetKind === 'external' ? i.package : i.targetKind}</span>
                    </div>
                  ))}
                </div>
              )}
              {tab === 'refs' && (
                <div className="list">
                  {d.referencedBy.length === 0 && <span className="faint small">No project file imports this file{analysis.meta.dependenciesAnalyzed ? '' : ' (dependency analysis was not run)'}.</span>}
                  {d.referencedBy.map((r, k) => (
                    <div key={k} className="li clickable" onClick={() => onSelect?.(r.path)}>
                      <span className="path ellipsis" style={{ flex: 1 }}>{r.path}</span>
                      <span className="faint xs">:{r.line}</span>
                    </div>
                  ))}
                </div>
              )}
              {tab === 'symbols' && (
                <div className="list">
                  {d.symbolsNote && <div className="banner info xs">{d.symbolsNote}</div>}
                  {d.symbols.length > 0 && <div className="faint xs">Lexical extraction — names and lines, no type information.</div>}
                  {d.symbols.map((s, k) => (
                    <div key={k} className="li">
                      <Icon name={KIND_ICON[s.kind] || 'code'} size={14} />
                      <span className="mono small ellipsis" style={{ flex: 1 }}>{s.name}</span>
                      {s.exported && <span className="badge">exported</span>}
                      <span className="faint xs">{s.kind} :{s.line}</span>
                    </div>
                  ))}
                  {d.file.exports.length > 0 && (
                    <div>
                      <div className="section-title" style={{ marginTop: 8 }}>Exports</div>
                      <div className="row wrap">
                        {d.file.exports.map((e) => (
                          <span key={e} className="chip mono">{e}</span>
                        ))}
                      </div>
                    </div>
                  )}
                </div>
              )}
              {tab === 'history' && (
                <div className="list">
                  {d.gitNote && <span className="faint small">{d.gitNote}</span>}
                  {d.git.map((c) => (
                    <div key={c.hash} className="li">
                      <Icon name="git" size={14} />
                      <div style={{ minWidth: 0, flex: 1 }}>
                        <div className="ellipsis small">{c.subject}</div>
                        <div className="faint xs">
                          {c.author} · {relative(c.timestamp)} · <span className="mono">{c.hash.slice(0, 8)}</span>
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              )}
              {tab === 'notes' && <AnnotationEditor projectId={project.id} path={path} initial={d.annotation} onSaved={() => toast('Annotation saved')} />}

              {d.findings.length > 0 && (
                <div>
                  <div className="section-title">Findings</div>
                  <div className="list">
                    {d.findings.map((f, k) => (
                      <div key={k} className="li clickable" onClick={() => navigate('health', { rule: f.rule })}>
                        <SeverityBadge severity={f.severity} />
                        <span className="ellipsis small">{f.title}</span>
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </>
          )}
        </Async>
      </div>
    </aside>
  );
}

export function AnnotationEditor({ projectId, path, initial, onSaved }) {
  const { toast } = useApp();
  const [a, setA] = useState(initial || { path, label: '', note: '', important: false, entryPoint: false, groupId: null });
  useEffect(() => setA(initial || { path, label: '', note: '', important: false, entryPoint: false, groupId: null }), [initial, path]);
  const save = async () => {
    try {
      await api.annotationSet(projectId, { ...a, path });
      onSaved?.();
    } catch (e) {
      toast(errorMessage(e), 'error');
    }
  };
  return (
    <div className="stack">
      <input className="input" placeholder="Label (e.g. Payment core)" value={a.label} maxLength={200} onChange={(e) => setA({ ...a, label: e.target.value })} />
      <textarea className="textarea" placeholder="Architectural note…" value={a.note} onChange={(e) => setA({ ...a, note: e.target.value })} />
      <div className="row wrap" style={{ gap: 16 }}>
        <Check checked={a.important} onChange={(v) => setA({ ...a, important: v })}>
          Important
        </Check>
        <Check checked={a.entryPoint} onChange={(v) => setA({ ...a, entryPoint: v })}>
          Entry point
        </Check>
      </div>
      <div className="faint xs">Annotations are stored by path in Structura's database and survive re-analysis. Entry points are taken into account by the "unused module" rule on the next analysis.</div>
      <div>
        <button type="button" className="btn primary sm" onClick={save}>
          Save annotation
        </button>
      </div>
    </div>
  );
}

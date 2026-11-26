import { useCallback, useEffect, useMemo, useState } from 'react';
import { useApp } from '../app/store.jsx';
import { api, errorMessage } from '../lib/api.js';
import { useAsync, useDebounced } from '../lib/hooks.js';
import { bytes, num, ROLE_LABEL } from '../lib/format.js';
import { Async, Check, Empty, SearchInput } from '../components/ui.jsx';
import FilePanel from '../components/FilePanel.jsx';
import Icon from '../components/Icon.jsx';

function TreeNode({ item, depth, ctx }) {
  const [open, setOpen] = useState(ctx.expandTo?.startsWith(`${item.path}/`) || ctx.expandTo === item.path);
  const [children, setChildren] = useState(null);
  const [error, setError] = useState(null);
  const load = useCallback(() => {
    api
      .explorerList(ctx.projectId, ctx.analysisId, item.path, ctx.showHidden, ctx.showIgnored)
      .then(setChildren)
      .catch(setError);
  }, [item.path, ctx.projectId, ctx.analysisId, ctx.showHidden, ctx.showIgnored]);
  useEffect(() => {
    if (open && item.isDir) load();
  }, [open, item.isDir, load]);
  const dim = item.ignored || item.excluded || item.hidden;
  const selected = ctx.selected === item.path;
  const atLimit = depth >= ctx.maxDepth;
  return (
    <>
      <div
        className={`tree-row ${selected ? 'selected' : ''} ${dim ? 'dim' : ''}`}
        style={{ paddingLeft: 6 + depth * 14 }}
        onClick={() => {
          if (item.isDir && !item.symlink && !atLimit) setOpen(!open);
          ctx.onSelect(item);
        }}
        role="treeitem"
        aria-expanded={item.isDir ? open : undefined}
        aria-selected={selected}
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === 'Enter') ctx.onSelect(item);
          if (e.key === 'ArrowRight' && item.isDir) setOpen(true);
          if (e.key === 'ArrowLeft' && item.isDir) setOpen(false);
        }}
        title={[item.path, item.ignored && 'git-ignored', item.excluded && 'excluded by profile', item.symlink && 'symlink (not followed)', item.sensitive && 'sensitive (not read)'].filter(Boolean).join(' · ')}
      >
        <span className={`twisty ${open ? 'open' : ''}`}>{item.isDir && !item.symlink ? <Icon name="chevron" size={11} /> : null}</span>
        <Icon name={item.isDir ? (open ? 'folderOpen' : 'folder') : item.sensitive ? 'lock' : item.symlink ? 'link' : 'file'} size={14} />
        <span className="name">{item.name}</span>
        {item.important && <Icon name="star" size={12} />}
        <span className="meta">
          {item.label && <span className="chip" style={{ padding: '0 6px' }}>{item.label}</span>}
          {item.findings > 0 && <span className="dot medium" title={`${item.findings} findings`} />}
          {item.ignored && <span>ignored</span>}
          {item.excluded && <span>excluded</span>}
          {item.isDir ? item.childCount != null && <span>{num(item.childCount)}</span> : <span>{item.lines != null ? `${num(item.lines)} ln` : bytes(item.size)}</span>}
        </span>
      </div>
      {open && error && <div className="faint xs" style={{ paddingLeft: 26 + depth * 14 }}>{errorMessage(error)}</div>}
      {open && !children && !error && <div className="faint xs" style={{ paddingLeft: 26 + depth * 14 }}>…</div>}
      {open && children?.map((c) => <TreeNode key={c.path} item={c} depth={depth + 1} ctx={ctx} />)}
      {open && children?.length === 0 && <div className="faint xs" style={{ paddingLeft: 26 + depth * 14 }}>empty</div>}
    </>
  );
}

const SORTS = [
  { id: 'path', label: 'Path' },
  { id: 'size', label: 'Size' },
  { id: 'lines', label: 'Lines' },
  { id: 'complexity', label: 'Complexity' },
  { id: 'fanIn', label: 'Used by' },
  { id: 'fanOut', label: 'Uses' },
  { id: 'modified', label: 'Modified' },
];

export default function Explorer({ params }) {
  const { project, analysis, settings, saveSettings } = useApp();
  const [selected, setSelected] = useState(params?.file || null);
  const [dir, setDir] = useState(params?.path || '');
  const [search, setSearch] = useState('');
  const [role, setRole] = useState('');
  const [language, setLanguage] = useState(params?.language || '');
  const [ext, setExt] = useState('');
  const [sort, setSort] = useState(params?.sort || 'path');
  const [desc, setDesc] = useState(!!params?.sort);
  const [treeKey, setTreeKey] = useState(0);
  const q = useDebounced(search, 180);

  const root = useAsync(() => api.explorerList(project.id, analysis?.id, '', settings.explorerShowHidden, settings.explorerShowIgnored), [project.id, analysis?.id, settings.explorerShowHidden, settings.explorerShowIgnored, treeKey]);
  const files = useAsync(
    () =>
      analysis
        ? api.files(analysis.id, { search: q, roles: role ? [role] : [], languages: language ? [language] : [], extension: ext || null, dir: dir || null, sort, desc, limit: 500 })
        : Promise.resolve(null),
    [analysis?.id, q, role, language, ext, dir, sort, desc],
  );
  const overview = useAsync(() => (analysis ? api.overview(analysis.id) : Promise.resolve(null)), [analysis?.id]);

  const ctx = useMemo(
    () => ({
      projectId: project.id,
      analysisId: analysis?.id,
      showHidden: settings.explorerShowHidden,
      showIgnored: settings.explorerShowIgnored,
      maxDepth: settings.explorerMaxDepth,
      selected: selected || dir,
      expandTo: params?.file || params?.path,
      onSelect: (item) => {
        if (item.isDir) setDir(item.path);
        else setSelected(item.path);
      },
    }),
    [project.id, analysis?.id, settings, selected, dir, params],
  );

  const toggle = (key) => (v) => saveSettings({ ...settings, [key]: v });
  const sortBy = (id) => {
    if (sort === id) setDesc(!desc);
    else {
      setSort(id);
      setDesc(id !== 'path');
    }
  };

  return (
    <div className="page full">
      <div className="page-header">
        <div>
          <h1>Explorer</h1>
          <div className="sub">Live project tree with analysis overlay. Hidden, git-ignored and excluded entries can be shown for transparency.</div>
        </div>
        <div className="actions">
          <Check checked={settings.explorerShowHidden} onChange={toggle('explorerShowHidden')}>
            Hidden
          </Check>
          <Check checked={settings.explorerShowIgnored} onChange={toggle('explorerShowIgnored')}>
            Ignored / excluded
          </Check>
          <button type="button" className="btn sm" onClick={() => setTreeKey((k) => k + 1)} title="Reload tree">
            <Icon name="refresh" size={14} />
          </button>
        </div>
      </div>
      <div className={`split ${selected && analysis ? 'three' : ''}`}>
        <div className="pane card flush">
          <div className="row between" style={{ padding: '10px 12px', borderBottom: '1px solid var(--border)' }}>
            <span className="small" style={{ fontWeight: 600 }}>{project.name}</span>
            {dir && (
              <button type="button" className="btn ghost sm" onClick={() => setDir('')}>
                Clear folder filter
              </button>
            )}
          </div>
          <div className="pane-body tree" role="tree" style={{ padding: 6 }}>
            <Async state={root}>{(items) => items.map((i) => <TreeNode key={`${treeKey}-${i.path}`} item={i} depth={0} ctx={ctx} />)}</Async>
          </div>
        </div>

        <div className="pane card flush">
          {!analysis ? (
            <Empty icon="play" title="No analysis yet">
              The tree on the left is live. Analyse the project to search, sort and inspect files with line counts, dependencies and symbols.
            </Empty>
          ) : (
            <>
              <div className="toolbar" style={{ padding: '10px 12px 0' }}>
                <SearchInput value={search} onChange={setSearch} placeholder="Search paths…" style={{ flex: 1, minWidth: 180 }} />
                <select className="select" value={role} onChange={(e) => setRole(e.target.value)} aria-label="Role filter">
                  <option value="">All roles</option>
                  {Object.entries(ROLE_LABEL).map(([k, v]) => (
                    <option key={k} value={k}>
                      {v}
                    </option>
                  ))}
                </select>
                <select className="select" value={language} onChange={(e) => setLanguage(e.target.value)} aria-label="Language filter">
                  <option value="">All languages</option>
                  {overview.data?.languages.map((l) => (
                    <option key={l.id} value={l.id}>
                      {l.name} ({l.files})
                    </option>
                  ))}
                </select>
                <input className="input" style={{ width: 80 }} placeholder=".ext" value={ext} onChange={(e) => setExt(e.target.value)} aria-label="Extension filter" />
              </div>
              <div className="faint xs" style={{ padding: '6px 12px' }}>
                {files.data ? `${num(files.data.total)} files${dir ? ` in ${dir}/` : ''}${files.data.total > files.data.rows.length ? ` · showing first ${files.data.rows.length}` : ''}` : ''}
              </div>
              <div className="pane-body table-wrap">
                <Async state={files}>
                  {(page) =>
                    page.rows.length === 0 ? (
                      <Empty icon="search" title="No matching files" />
                    ) : (
                      <table className="data">
                        <thead>
                          <tr>
                            {SORTS.filter((s) => s.id !== 'modified').map((s) => (
                              <th key={s.id} className={`sortable ${s.id === 'path' ? '' : 'r'}`} onClick={() => sortBy(s.id)} aria-sort={sort === s.id ? (desc ? 'descending' : 'ascending') : 'none'}>
                                {s.label} {sort === s.id ? (desc ? '↓' : '↑') : ''}
                              </th>
                            ))}
                          </tr>
                        </thead>
                        <tbody>
                          {page.rows.map((r) => (
                            <tr key={r.path} className={`clickable ${selected === r.path ? 'selected' : ''}`} onClick={() => setSelected(r.path)}>
                              <td style={{ maxWidth: 380 }}>
                                <div className="ellipsis" title={r.path}>
                                  {r.sensitive && <Icon name="lock" size={12} />} {r.name}
                                </div>
                                <div className="path ellipsis">{r.path}</div>
                              </td>
                              <td className="r num faint">{bytes(r.size)}</td>
                              <td className="r num">{r.lines != null ? num(r.lines) : '—'}</td>
                              <td className="r num">{r.complexity ?? '—'}</td>
                              <td className="r num">{analysis.meta.dependenciesAnalyzed ? r.fanIn : '—'}</td>
                              <td className="r num">{analysis.meta.dependenciesAnalyzed ? r.fanOut : '—'}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    )
                  }
                </Async>
              </div>
            </>
          )}
        </div>
        {selected && analysis && <FilePanel path={selected} onSelect={setSelected} onClose={() => setSelected(null)} />}
      </div>
      {params?.file && !analysis && <div className="faint xs">Analyse the project to inspect files.</div>}
    </div>
  );
}

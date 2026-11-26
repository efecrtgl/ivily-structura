import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import Icon from './Icon.jsx';
import { inTauri } from '../lib/api.js';
import logo from '../../assets/logo.svg';

/** Custom window frame: drag region + window controls (the native frame is disabled). */
export default function TitleBar({ crumb }) {
  const [maximized, setMaximized] = useState(false);
  const win = inTauri() ? getCurrentWindow() : null;

  useEffect(() => {
    if (!win) return undefined;
    win.isMaximized().then(setMaximized).catch(() => {});
    const un = win.onResized(() => win.isMaximized().then(setMaximized).catch(() => {}));
    return () => {
      un.then((f) => f()).catch(() => {});
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const onMouseDown = (e) => {
    if (e.button !== 0 || !win) return;
    if (e.detail === 2) win.toggleMaximize();
    else win.startDragging();
  };

  return (
    <header className="titlebar">
      <div className="drag" onMouseDown={onMouseDown}>
        <span className="app-mark">
          <img src={logo} alt="" />
          Ivily Structura
        </span>
        {crumb && <span className="crumb">{crumb}</span>}
      </div>
      <div className="win-controls">
        <button type="button" aria-label="Minimize" onClick={() => win?.minimize()}>
          <Icon name="minimize" size={14} />
        </button>
        <button type="button" aria-label={maximized ? 'Restore' : 'Maximize'} onClick={() => win?.toggleMaximize()}>
          <Icon name={maximized ? 'restore' : 'maximize'} size={12} />
        </button>
        <button type="button" className="close" aria-label="Close" onClick={() => win?.close()}>
          <Icon name="x" size={14} />
        </button>
      </div>
    </header>
  );
}

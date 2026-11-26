import { useEffect, useState } from 'react';
import { useApp } from '../app/store.jsx';
import { api } from '../lib/api.js';
import Icon from './Icon.jsx';

/** Profile picker + Analyse button. */
export default function AnalyzeControl({ primary = true }) {
  const { project, analyze, job, settings } = useApp();
  const [profiles, setProfiles] = useState([]);
  const [profile, setProfile] = useState(null);
  useEffect(() => {
    api.profileList().then(setProfiles).catch(() => {});
  }, []);
  useEffect(() => {
    setProfile(project?.profileId || settings?.defaultProfile || 'full');
  }, [project?.profileId, settings?.defaultProfile]);
  const running = job && job.projectId === project?.id;
  const current = profiles.find((p) => p.id === profile);
  return (
    <div className="row">
      <select className="select" value={profile || ''} onChange={(e) => setProfile(e.target.value)} aria-label="Analysis profile" title={current?.description}>
        {profiles.map((p) => (
          <option key={p.id} value={p.id}>
            {p.name}
          </option>
        ))}
      </select>
      <button type="button" className={`btn ${primary ? 'primary' : ''}`} disabled={!project || running || !project.exists} onClick={() => analyze(profile)}>
        <Icon name={running ? 'refresh' : 'play'} />
        {running ? 'Analysing…' : 'Analyse'}
      </button>
    </div>
  );
}

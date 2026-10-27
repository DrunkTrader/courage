import { useEffect, useMemo, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'

type View = 'library' | 'recording' | 'settings'

type Clip = {
  id: string
  title: string
  subtitle: string
  duration: string
  accent: string
}

type EngineStatus = {
  state: 'ready' | 'capturing' | 'permission-required' | 'unavailable'
  platform: string
  backend: string
}

const demoClips: Clip[] = [
  { id: 'demo-01', title: 'Pipeline demo', subtitle: 'Synthetic capture · H.264 + AAC', duration: '00:04', accent: 'sunset' },
  { id: 'demo-02', title: 'Rolling buffer preview', subtitle: 'Last 30 seconds · 1080p', duration: '00:30', accent: 'violet' },
  { id: 'demo-03', title: 'Desk warm-up', subtitle: 'Screenshot · Today, 09:42', duration: '—', accent: 'steel' },
]

export function App() {
  const [view, setView] = useState<View>('library')
  const [isRecording, setIsRecording] = useState(false)
  const [replaySeconds, setReplaySeconds] = useState(30)
  const [hotkey, setHotkey] = useState('⌘ ⇧ 1')
  const [status, setStatus] = useState<EngineStatus>({ state: 'ready', platform: 'browser preview', backend: 'synthetic source' })
  const [message, setMessage] = useState('Ready when you are.')

  useEffect(() => {
    if (!('__TAURI_INTERNALS__' in window)) return
    invoke<EngineStatus>('engine_status')
      .then(setStatus)
      .catch(() => setStatus({ state: 'unavailable', platform: 'desktop', backend: 'engine unavailable' }))
  }, [])

  const statusLabel = useMemo(() => {
    if (isRecording) return 'Recording session'
    if (status.state === 'permission-required') return 'Permission needed'
    if (status.state === 'unavailable') return 'Engine unavailable'
    return 'Replay buffer armed'
  }, [isRecording, status.state])

  function toggleRecording() {
    setIsRecording((current) => !current)
    setMessage(isRecording ? 'Session saved to the local library.' : 'Full-session recording started.')
  }

  async function saveReplay() {
    if ('__TAURI_INTERNALS__' in window) {
      try {
        await invoke('save_replay')
        setMessage(`Replay saved — last ${replaySeconds} seconds.`)
      } catch (error) {
        setMessage(`Replay unavailable: ${String(error)}`)
      }
      return
    }
    setMessage(`Preview request queued — last ${replaySeconds} seconds.`)
  }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand-lockup">
          <div className="brand-mark" aria-hidden="true">C</div>
          <div>
            <strong>Courage</strong>
            <span>have it.</span>
          </div>
        </div>

        <nav className="primary-nav" aria-label="Primary navigation">
          <NavButton active={view === 'library'} onClick={() => setView('library')} icon="▦">Library</NavButton>
          <NavButton active={view === 'recording'} onClick={() => setView('recording')} icon="◉">Capture</NavButton>
          <NavButton active={view === 'settings'} onClick={() => setView('settings')} icon="⚙">Settings</NavButton>
        </nav>

        <div className="sidebar-foot">
          <div className="local-pill"><span className="pulse-dot" />Local only</div>
          <p>Private by default.<br />Your moments stay yours.</p>
          <small>v0.1 · engine preview</small>
        </div>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <span className="eyebrow">{view === 'library' ? 'Your moments' : view === 'recording' ? 'Capture desk' : 'Control room'}</span>
            <h1>{view === 'library' ? 'Library' : view === 'recording' ? 'Capture' : 'Settings'}</h1>
          </div>
          <div className={`engine-state ${status.state}`}><span />{statusLabel}</div>
        </header>

        {view === 'library' && (
          <div className="content-grid">
            <section className="hero-panel">
              <div className="hero-copy">
                <span className="eyebrow warm">Instant replay</span>
                <h2>Play the moment<br /><em>you meant to keep.</em></h2>
                <p>Capture stays rolling in the background. Press your hotkey when the impossible happens.</p>
                <div className="hero-actions">
                  <button className="primary-button" onClick={saveReplay}><span>◆</span> Save last {replaySeconds}s <kbd>{hotkey}</kbd></button>
                  <button className="quiet-button" onClick={() => setView('settings')}>Configure</button>
                </div>
              </div>
              <div className="hero-orbit" aria-hidden="true">
                <div className="orbit orbit-one" /><div className="orbit orbit-two" /><div className="orbit-core">30<span>s</span></div>
              </div>
            </section>

            <section className="quick-panel">
              <div className="section-heading"><span>Session recorder</span><span className="signal">● Live ready</span></div>
              <div className="record-control"><div className={`record-ring ${isRecording ? 'active' : ''}`}><button aria-label={isRecording ? 'Stop recording' : 'Start recording'} onClick={toggleRecording}><span /></button></div></div>
              <strong>{isRecording ? 'Recording everything' : 'Record a full session'}</strong>
              <p>{isRecording ? 'Courage is capturing your entire session.' : 'Keep the whole run when 30 seconds is not enough.'}</p>
              <button className="outline-button" onClick={toggleRecording}>{isRecording ? 'Stop recording' : 'Start recording'}</button>
            </section>

            <section className="library-section">
              <div className="section-heading"><div><span className="eyebrow">Local library</span><h3>Recent captures</h3></div><button className="text-button">View all <span>→</span></button></div>
              <div className="clip-grid">{demoClips.map((clip) => <ClipCard key={clip.id} clip={clip} />)}</div>
            </section>
          </div>
        )}

        {view === 'recording' && <CaptureView isRecording={isRecording} onToggle={toggleRecording} message={message} />}
        {view === 'settings' && <SettingsView replaySeconds={replaySeconds} setReplaySeconds={setReplaySeconds} hotkey={hotkey} setHotkey={setHotkey} status={status} />}

        <footer className="status-line" aria-live="polite"><span className="status-check">✓</span>{message}<span className="status-meta">{status.platform} · {status.backend}</span></footer>
      </section>
    </main>
  )
}

function NavButton({ active, onClick, icon, children }: { active: boolean; onClick: () => void; icon: string; children: string }) {
  return <button className={`nav-button ${active ? 'active' : ''}`} onClick={onClick}><span>{icon}</span>{children}</button>
}

function ClipCard({ clip }: { clip: Clip }) {
  return <article className="clip-card"><div className={`clip-thumb ${clip.accent}`}><span className="play-icon">▶</span><span className="clip-duration">{clip.duration}</span></div><div className="clip-info"><strong>{clip.title}</strong><span>{clip.subtitle}</span></div><button className="more-button" aria-label={`More options for ${clip.title}`}>•••</button></article>
}

function CaptureView({ isRecording, onToggle, message }: { isRecording: boolean; onToggle: () => void; message: string }) {
  return <section className="detail-view"><div className="detail-header"><span className="eyebrow warm">Capture status</span><h2>{isRecording ? 'Your session is live.' : 'A quiet buffer, always ready.'}</h2><p>{message}</p></div><div className="capture-card"><div className={`big-record-indicator ${isRecording ? 'active' : ''}`} /><div><strong>{isRecording ? 'Recording full session' : 'Instant replay armed'}</strong><p>{isRecording ? 'Video, system audio, and microphone routing will appear here when native capture is connected.' : 'The local rolling buffer is waiting for a moment worth keeping.'}</p></div><button className="primary-button" onClick={onToggle}>{isRecording ? 'Stop recording' : 'Start recording'}</button></div><div className="info-grid"><InfoBlock title="Video source" value="Native backend pending" /><InfoBlock title="Audio mix" value="System + microphone" /><InfoBlock title="Storage" value="Local disk only" /></div></section>
}

function InfoBlock({ title, value }: { title: string; value: string }) { return <div className="info-block"><span>{title}</span><strong>{value}</strong></div> }

function SettingsView({ replaySeconds, setReplaySeconds, hotkey, setHotkey, status }: { replaySeconds: number; setReplaySeconds: (value: number) => void; hotkey: string; setHotkey: (value: string) => void; status: EngineStatus }) {
  return <section className="detail-view settings-view"><div className="detail-header"><span className="eyebrow warm">Control room</span><h2>Make the recorder<br /><em>feel like yours.</em></h2><p>Settings are local and take effect without an account.</p></div><div className="settings-list"><label className="setting-row"><span><strong>Replay length</strong><small>How far back the save action reaches</small></span><span className="range-control"><output>{replaySeconds}s</output><input type="range" min="5" max="120" step="5" value={replaySeconds} onChange={(event) => setReplaySeconds(Number(event.target.value))} /></span></label><label className="setting-row"><span><strong>Save replay hotkey</strong><small>Global shortcut used by the native desktop build</small></span><input className="hotkey-input" value={hotkey} onChange={(event) => setHotkey(event.target.value)} aria-label="Save replay hotkey" /></label><div className="setting-row"><span><strong>Engine capability</strong><small>Current runtime report</small></span><span className={`capability ${status.state}`}><i />{status.backend}</span></div></div></section>
}

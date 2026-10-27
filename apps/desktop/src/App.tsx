import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { convertFileSrc, invoke } from '@tauri-apps/api/core'

type View = 'library' | 'recording' | 'settings'

type Clip = {
  id: number
  path: string
  title: string
  durationMs: number
  createdAt: number
}

type EngineStatus = {
  state: 'ready' | 'capturing' | 'permission-required' | 'unavailable'
  platform: string
  backend: string
  voiceTrigger: string
  replaySeconds: number
}

type RecognitionEvent = { results: ArrayLike<ArrayLike<{ transcript: string }>> }
type Recognition = {
  continuous: boolean
  interimResults: boolean
  lang: string
  onresult: ((event: RecognitionEvent) => void) | null
  onerror: (() => void) | null
  onend: (() => void) | null
  start: () => void
  stop: () => void
}

type RecognitionConstructor = new () => Recognition

const emptyStatus: EngineStatus = {
  state: 'unavailable',
  platform: 'browser preview',
  backend: 'Desktop runtime required',
  voiceTrigger: 'unavailable',
  replaySeconds: 30,
}

export function App() {
  const [view, setView] = useState<View>('library')
  const [isRecording, setIsRecording] = useState(false)
  const [replaySeconds, setReplaySeconds] = useState(30)
  const [hotkey, setHotkey] = useState('⌘ ⇧ 1')
  const [voiceEnabled, setVoiceEnabled] = useState(true)
  const [voiceSupported, setVoiceSupported] = useState(false)
  const [status, setStatus] = useState<EngineStatus>(emptyStatus)
  const [clips, setClips] = useState<Clip[]>([])
  const [selectedClip, setSelectedClip] = useState<Clip | null>(null)
  const [message, setMessage] = useState('Starting the replay buffer…')

  const refreshClips = useCallback(async () => {
    if (!('__TAURI_INTERNALS__' in window)) {
      setClips([])
      return
    }
    try {
      setClips(await invoke<Clip[]>('list_clips'))
    } catch (error) {
      setMessage(`Could not load clips: ${String(error)}`)
    }
  }, [])

  const saveReplay = useCallback(async (source: 'button' | 'voice' = 'button') => {
    if (!('__TAURI_INTERNALS__' in window)) {
      setMessage('Run the Tauri desktop app to record. Browser preview cannot capture your desktop.')
      return
    }
    try {
      const clip = await invoke<Clip>('save_replay')
      setClips((current) => [clip, ...current.filter((item) => item.id !== clip.id)])
      setSelectedClip(clip)
      setMessage(source === 'voice' ? 'Courage, have it — replay saved.' : 'Replay saved to your local library.')
    } catch (error) {
      setMessage(String(error))
    }
  }, [])

  useEffect(() => {
    if (!('__TAURI_INTERNALS__' in window)) {
      setMessage('Desktop runtime required for capture.')
      return
    }
    invoke<EngineStatus>('start_capture', { replaySeconds })
      .then((nextStatus) => {
        setStatus(nextStatus)
        setIsRecording(nextStatus.state === 'capturing')
        setMessage(nextStatus.state === 'capturing' ? 'Replay buffer armed.' : 'Replay buffer is ready.')
      })
      .catch((error) => {
        setStatus({ ...emptyStatus, state: 'unavailable', backend: String(error) })
        setMessage(String(error))
      })
    void refreshClips()
  }, [refreshClips, replaySeconds])

  useWakePhrase({
    enabled: voiceEnabled && voiceSupported && isRecording,
    onPhrase: () => void saveReplay('voice'),
    onSupport: setVoiceSupported,
    onMessage: setMessage,
  })

  const statusLabel = useMemo(() => {
    if (isRecording) return voiceEnabled && voiceSupported ? 'Listening · buffer armed' : 'Replay buffer armed'
    return status.state === 'unavailable' ? 'Capture unavailable' : 'Capture stopped'
  }, [isRecording, status.state, voiceEnabled, voiceSupported])

  async function toggleRecording() {
    if (!('__TAURI_INTERNALS__' in window)) {
      setMessage('Run the Tauri desktop app to record. Browser preview cannot capture your desktop.')
      return
    }
    try {
      if (isRecording) {
        const nextStatus = await invoke<EngineStatus>('stop_capture')
        setStatus(nextStatus)
        setIsRecording(false)
        setMessage('Replay buffer stopped.')
      } else {
        const nextStatus = await invoke<EngineStatus>('start_capture', { replaySeconds })
        setStatus(nextStatus)
        setIsRecording(true)
        setMessage('Replay buffer armed.')
      }
    } catch (error) {
      setMessage(String(error))
    }
  }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand-lockup">
          <div className="brand-mark" aria-hidden="true">C</div>
          <div><strong>Courage</strong><span>have it.</span></div>
        </div>

        <nav className="primary-nav" aria-label="Primary navigation">
          <NavButton active={view === 'library'} onClick={() => setView('library')} icon="▦">Library</NavButton>
          <NavButton active={view === 'recording'} onClick={() => setView('recording')} icon="◉">Capture</NavButton>
          <NavButton active={view === 'settings'} onClick={() => setView('settings')} icon="⚙">Settings</NavButton>
        </nav>

        <div className="sidebar-foot">
          <div className="local-pill"><span className="pulse-dot" />Local only</div>
          <p>Your clips stay on this machine.<br />No demo content is included.</p>
          <small>v0.1 · local capture</small>
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
                <h2>Say the words.<br /><em>Keep the moment.</em></h2>
                <p>{voiceEnabled && voiceSupported ? 'Say “Courage, have it” at any moment. Courage saves the last 30 seconds.' : 'Press save when something happens. Courage keeps the last 30 seconds locally.'}</p>
                <div className="hero-actions">
                  <button className="primary-button" onClick={() => void saveReplay()}><span>◆</span> Save last {replaySeconds}s <kbd>{hotkey}</kbd></button>
                  <button className="quiet-button" onClick={() => setView('settings')}>Configure</button>
                </div>
              </div>
              <div className="hero-orbit" aria-hidden="true"><div className="orbit orbit-one" /><div className="orbit orbit-two" /><div className="orbit-core">{replaySeconds}<span>s</span></div></div>
            </section>

            <section className="quick-panel">
              <div className="section-heading"><span>Voice trigger</span><span className="signal">{voiceEnabled && voiceSupported ? '● Listening' : '○ Off'}</span></div>
              <div className="record-control"><div className={`record-ring ${isRecording ? 'active' : ''}`}><button aria-label={isRecording ? 'Stop replay buffer' : 'Start replay buffer'} onClick={() => void toggleRecording()}><span /></button></div></div>
              <strong>{isRecording ? 'Replay buffer is live' : 'Replay buffer is stopped'}</strong>
              <p>{voiceEnabled && voiceSupported ? 'Say “Courage, have it” to save the last 30 seconds.' : 'Enable voice trigger in Settings or use the button.'}</p>
              <button className="outline-button" onClick={() => void toggleRecording()}>{isRecording ? 'Stop buffer' : 'Start buffer'}</button>
            </section>

            <section className="library-section">
              <div className="section-heading"><div><span className="eyebrow">Local library</span><h3>Recent captures</h3></div><button className="text-button" onClick={() => void refreshClips()}>Refresh <span>↻</span></button></div>
              {clips.length === 0 ? <EmptyLibrary onSave={() => void saveReplay()} /> : <div className="clip-grid">{clips.map((clip) => <ClipCard key={clip.id} clip={clip} selected={selectedClip?.id === clip.id} onSelect={setSelectedClip} />)}</div>}
            </section>
            {selectedClip && <ClipPlayer clip={selectedClip} onClose={() => setSelectedClip(null)} />}
          </div>
        )}

        {view === 'recording' && <CaptureView isRecording={isRecording} onToggle={() => void toggleRecording()} message={message} voiceEnabled={voiceEnabled} voiceSupported={voiceSupported} />}
        {view === 'settings' && <SettingsView replaySeconds={replaySeconds} setReplaySeconds={setReplaySeconds} hotkey={hotkey} setHotkey={setHotkey} voiceEnabled={voiceEnabled} setVoiceEnabled={setVoiceEnabled} voiceSupported={voiceSupported} status={status} />}

        <footer className="status-line" aria-live="polite"><span className="status-check">✓</span>{message}<span className="status-meta">{status.platform} · {status.backend}</span></footer>
      </section>
    </main>
  )
}

function useWakePhrase({ enabled, onPhrase, onSupport, onMessage }: { enabled: boolean; onPhrase: () => void; onSupport: (supported: boolean) => void; onMessage: (message: string) => void }) {
  const recognitionRef = useRef<Recognition | null>(null)
  const onPhraseRef = useRef(onPhrase)
  onPhraseRef.current = onPhrase

  useEffect(() => {
    const recognitionConstructor = (window as unknown as { SpeechRecognition?: RecognitionConstructor; webkitSpeechRecognition?: RecognitionConstructor }).SpeechRecognition
      ?? (window as unknown as { webkitSpeechRecognition?: RecognitionConstructor }).webkitSpeechRecognition
    if (!recognitionConstructor) {
      onSupport(false)
      return
    }
    onSupport(true)
    const recognition = new recognitionConstructor()
    recognition.continuous = true
    recognition.interimResults = false
    recognition.lang = 'en-US'
    recognition.onresult = (event) => {
      const transcript = Array.from(event.results).map((result) => result[0]?.transcript ?? '').join(' ').toLowerCase()
      const normalized = transcript.replace(/[^a-z0-9 ]/g, ' ').replace(/\s+/g, ' ').trim()
      if (normalized.includes('courage have it')) onPhraseRef.current()
    }
    recognition.onerror = () => onMessage('Voice trigger needs microphone permission or is unavailable in this WebView.')
    let active = true
    recognition.onend = () => {
      if (active && enabled) {
        try { recognition.start() } catch { /* browser rejects overlapping starts */ }
      }
    }
    recognitionRef.current = recognition
    if (enabled) {
      try { recognition.start() } catch { onMessage('Voice trigger could not start. Check microphone permission.') }
    }
    return () => {
      active = false
      recognition.stop()
      recognitionRef.current = null
    }
  }, [enabled, onMessage, onSupport])
}

function EmptyLibrary({ onSave }: { onSave: () => void }) {
  return <div className="empty-library"><div className="empty-mark">◆</div><strong>No captures yet</strong><p>Your saved replays will appear here. Start the buffer, then say “Courage, have it”.</p><button className="outline-button" onClick={onSave}>Save a replay</button></div>
}

function ClipCard({ clip, selected, onSelect }: { clip: Clip; selected: boolean; onSelect: (clip: Clip) => void }) {
  return <article className={`clip-card ${selected ? 'selected' : ''}`}><button className="clip-thumb real" onClick={() => onSelect(clip)} aria-label={`Play ${clip.title}`}><span className="play-icon">▶</span><span className="clip-duration">{formatDuration(clip.durationMs)}</span></button><div className="clip-info"><strong>{clip.title}</strong><span>{new Date(clip.createdAt * 1000).toLocaleString()}</span></div></article>
}

function ClipPlayer({ clip, onClose }: { clip: Clip; onClose: () => void }) {
  const source = '__TAURI_INTERNALS__' in window ? convertFileSrc(clip.path) : undefined
  return <div className="clip-player" role="dialog" aria-label={`Playing ${clip.title}`}><div className="player-heading"><strong>{clip.title}</strong><button className="quiet-button" onClick={onClose}>Close</button></div>{source ? <video controls autoPlay src={source} /> : <p>Playback is available in the Tauri desktop app.</p>}</div>
}

function formatDuration(durationMs: number) {
  const seconds = Math.max(0, Math.round(durationMs / 1000))
  return `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`
}

function NavButton({ active, onClick, icon, children }: { active: boolean; onClick: () => void; icon: string; children: string }) { return <button className={`nav-button ${active ? 'active' : ''}`} onClick={onClick}><span>{icon}</span>{children}</button> }

function CaptureView({ isRecording, onToggle, message, voiceEnabled, voiceSupported }: { isRecording: boolean; onToggle: () => void; message: string; voiceEnabled: boolean; voiceSupported: boolean }) {
  return <section className="detail-view"><div className="detail-header"><span className="eyebrow warm">Capture status</span><h2>{isRecording ? 'Your replay buffer is live.' : 'Start the buffer before you play.'}</h2><p>{message}</p></div><div className="capture-card"><div className={`big-record-indicator ${isRecording ? 'active' : ''}`} /><div><strong>{isRecording ? 'Capturing the desktop now' : 'Capture is stopped'}</strong><p>{isRecording ? `Rolling video${voiceEnabled && voiceSupported ? ' and listening for “Courage, have it”' : ''}. Saved clips are written to the local library.` : 'No frames are being recorded until you start the buffer.'}</p></div><button className="primary-button" onClick={onToggle}>{isRecording ? 'Stop buffer' : 'Start buffer'}</button></div><div className="info-grid"><InfoBlock title="Video source" value="FFmpeg native desktop capture" /><InfoBlock title="Audio input" value="Configured microphone / system default" /><InfoBlock title="Storage" value="Local app data" /></div></section>
}

function InfoBlock({ title, value }: { title: string; value: string }) { return <div className="info-block"><span>{title}</span><strong>{value}</strong></div> }

function SettingsView({ replaySeconds, setReplaySeconds, hotkey, setHotkey, voiceEnabled, setVoiceEnabled, voiceSupported, status }: { replaySeconds: number; setReplaySeconds: (value: number) => void; hotkey: string; setHotkey: (value: string) => void; voiceEnabled: boolean; setVoiceEnabled: (value: boolean) => void; voiceSupported: boolean; status: EngineStatus }) {
  return <section className="detail-view settings-view"><div className="detail-header"><span className="eyebrow warm">Control room</span><h2>Make the recorder<br /><em>feel like yours.</em></h2><p>Capture stays local. The voice trigger only listens for the configured phrase while the buffer is armed.</p></div><div className="settings-list"><label className="setting-row"><span><strong>Replay length</strong><small>How far back the save action reaches</small></span><span className="range-control"><output>{replaySeconds}s</output><input type="range" min="5" max="120" step="5" value={replaySeconds} onChange={(event) => setReplaySeconds(Number(event.target.value))} /></span></label><label className="setting-row"><span><strong>Voice trigger</strong><small>Say “Courage, have it” to save a replay</small></span><input className="toggle-input" type="checkbox" checked={voiceEnabled} onChange={(event) => setVoiceEnabled(event.target.checked)} disabled={!voiceSupported} aria-label="Enable voice trigger" /></label><label className="setting-row"><span><strong>Save replay hotkey</strong><small>Fallback shortcut for the desktop build</small></span><input className="hotkey-input" value={hotkey} onChange={(event) => setHotkey(event.target.value)} aria-label="Save replay hotkey" /></label><div className="setting-row"><span><strong>Engine capability</strong><small>{status.voiceTrigger}</small></span><span className={`capability ${status.state}`}><i />{status.backend}</span></div></div></section>
}

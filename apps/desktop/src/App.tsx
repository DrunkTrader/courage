import { useCallback, useEffect, useMemo, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { register, unregister } from '@tauri-apps/plugin-global-shortcut'

type View = 'library' | 'capture' | 'settings'

type Clip = {
  id: number
  path: string
  title: string
  durationMs: number
  createdAt: number
}

type AudioDevice = {
  id: string
  name: string
  backend: string
  isDefault: boolean
}

type Settings = {
  outputDir: string
  defaultOutputDir: string
  audioDevices: AudioDevice[]
  selectedAudioDevice: string | null
  hotkey: string
  replaySeconds: number
  voiceEnabled: boolean
  nativeVoiceAvailable: boolean
}

type EngineStatus = {
  state: 'ready' | 'capturing' | 'unavailable'
  platform: string
  backend: string
  audioDevice: string | null
  voiceTrigger: string
  outputDir: string
  replaySeconds: number
}

const emptySettings: Settings = {
  outputDir: '',
  defaultOutputDir: '',
  audioDevices: [],
  selectedAudioDevice: null,
  hotkey: 'CommandOrControl+Shift+1',
  replaySeconds: 30,
  voiceEnabled: true,
  nativeVoiceAvailable: false,
}

const emptyStatus: EngineStatus = {
  state: 'unavailable',
  platform: 'browser preview',
  backend: 'Desktop runtime required',
  audioDevice: null,
  voiceTrigger: 'Native desktop runtime required',
  outputDir: '',
  replaySeconds: 30,
}

const hotkeyOptions = [
  { value: 'CommandOrControl+Shift+1', label: 'Ctrl / ⌘  ⇧  1' },
  { value: 'CommandOrControl+Shift+2', label: 'Ctrl / ⌘  ⇧  2' },
  { value: 'Alt+F10', label: 'Alt  F10' },
]

const isDesktop = () => '__TAURI_INTERNALS__' in window

export function App() {
  const [view, setView] = useState<View>('library')
  const [settings, setSettings] = useState<Settings>(emptySettings)
  const [draft, setDraft] = useState<Settings>(emptySettings)
  const [status, setStatus] = useState<EngineStatus>(emptyStatus)
  const [clips, setClips] = useState<Clip[]>([])
  const [selectedClip, setSelectedClip] = useState<Clip | null>(null)
  const [message, setMessage] = useState('Starting the local capture engine…')
  const [flash, setFlash] = useState<{ title: string; body: string } | null>(null)
  const [savingSettings, setSavingSettings] = useState(false)

  const showFlash = useCallback((title: string, body: string) => {
    setFlash({ title, body })
    window.setTimeout(() => setFlash(null), 4200)
  }, [])

  const refreshClips = useCallback(async () => {
    if (!isDesktop()) return
    try {
      setClips(await invoke<Clip[]>('list_clips'))
    } catch (error) {
      setMessage(`Library unavailable: ${String(error)}`)
    }
  }, [])

  const saveReplay = useCallback(async (trigger: 'button' | 'hotkey' | 'voice' = 'button') => {
    if (!isDesktop()) {
      setMessage('Run the Courage desktop build to capture the display.')
      return
    }
    try {
      const clip = await invoke<Clip>('save_replay')
      setClips((current) => [clip, ...current.filter((item) => item.id !== clip.id)])
      setSelectedClip(clip)
      const source = trigger === 'voice' ? 'Voice trigger' : trigger === 'hotkey' ? 'Hotkey' : 'Save action'
      setMessage(`${source} saved ${clip.title}.`)
      showFlash('Clip saved', clip.path)
    } catch (error) {
      setMessage(String(error))
      showFlash('Could not save clip', String(error))
    }
  }, [showFlash])

  useEffect(() => {
    if (!isDesktop()) {
      setMessage('Desktop runtime required for capture.')
      return
    }
    let disposed = false
    let unlistenClip: (() => void) | undefined
    let unlistenError: (() => void) | undefined

    const boot = async () => {
      try {
        const [loadedSettings] = await Promise.all([
          invoke<Settings>('get_settings'),
          refreshClips(),
        ])
        if (disposed) return
        setSettings(loadedSettings)
        setDraft(loadedSettings)
        const nextStatus = await invoke<EngineStatus>('start_capture')
        if (disposed) return
        setStatus(nextStatus)
        setMessage(nextStatus.state === 'capturing' ? 'Replay buffer armed. Courage is ready.' : 'Replay buffer is ready.')
      } catch (error) {
        if (!disposed) {
          setStatus({ ...emptyStatus, backend: String(error) })
          setMessage(String(error))
        }
      }
      unlistenClip = await listen<Clip>('clip-saved', (event) => {
        if (disposed) return
        const clip = event.payload
        setClips((current) => [clip, ...current.filter((item) => item.id !== clip.id)])
        setSelectedClip(clip)
        setMessage(`Saved ${clip.title}.`)
        showFlash('Clip saved', clip.path)
      })
      unlistenError = await listen<string>('capture-error', (event) => {
        if (!disposed) {
          setMessage(event.payload)
          showFlash('Capture needs attention', event.payload)
        }
      })
    }
    void boot()
    const statusTimer = window.setInterval(() => {
      void invoke<EngineStatus>('engine_status').then((nextStatus) => {
        if (!disposed) setStatus(nextStatus)
      }).catch(() => undefined)
    }, 1600)
    return () => {
      disposed = true
      window.clearInterval(statusTimer)
      unlistenClip?.()
      unlistenError?.()
    }
  }, [refreshClips, showFlash])

  useEffect(() => {
    if (!isDesktop() || !settings.hotkey) return
    let active = true
    void register(settings.hotkey, (event) => {
      if (active && event.state === 'Pressed') void saveReplay('hotkey')
    }).catch((error) => {
      if (active) setMessage(`Hotkey unavailable: ${String(error)}`)
    })
    return () => {
      active = false
      void unregister(settings.hotkey).catch(() => undefined)
    }
  }, [saveReplay, settings.hotkey])

  const isRecording = status.state === 'capturing'
  const hotkeyLabel = hotkeyOptions.find((option) => option.value === settings.hotkey)?.label ?? settings.hotkey
  const voiceReady = settings.voiceEnabled && settings.nativeVoiceAvailable && status.voiceTrigger.includes('listening')

  async function toggleCapture() {
    try {
      const nextStatus = isRecording
        ? await invoke<EngineStatus>('stop_capture')
        : await invoke<EngineStatus>('start_capture')
      setStatus(nextStatus)
      setMessage(nextStatus.state === 'capturing' ? 'Replay buffer armed.' : 'Replay buffer stopped.')
    } catch (error) {
      setMessage(String(error))
    }
  }

  async function refreshDevices() {
    try {
      const audioDevices = await invoke<AudioDevice[]>('list_audio_devices')
      setDraft((current) => ({ ...current, audioDevices }))
      setMessage(audioDevices.length ? `${audioDevices.length} recording device${audioDevices.length === 1 ? '' : 's'} found.` : 'No recording devices found.')
    } catch (error) {
      setMessage(String(error))
    }
  }

  async function chooseOutputDirectory() {
    const result = await open({
      directory: true,
      multiple: false,
      title: 'Choose where Courage saves clips',
      defaultPath: draft.outputDir || undefined,
    })
    if (typeof result === 'string') setDraft((current) => ({ ...current, outputDir: result }))
  }

  async function applySettings() {
    setSavingSettings(true)
    try {
      const nextSettings = await invoke<Settings>('save_settings', {
        settings: {
          outputDir: draft.outputDir,
          selectedAudioDevice: draft.selectedAudioDevice,
          hotkey: draft.hotkey,
          replaySeconds: draft.replaySeconds,
          voiceEnabled: draft.voiceEnabled,
        },
      })
      await invoke<EngineStatus>('stop_capture').catch(() => undefined)
      const nextStatus = await invoke<EngineStatus>('start_capture')
      setSettings(nextSettings)
      setDraft(nextSettings)
      setStatus(nextStatus)
      setMessage('Settings saved. The replay buffer is using the new configuration.')
    } catch (error) {
      setMessage(String(error))
    } finally {
      setSavingSettings(false)
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
          <NavButton active={view === 'capture'} onClick={() => setView('capture')} icon="◉">Capture</NavButton>
          <NavButton active={view === 'settings'} onClick={() => setView('settings')} icon="⚙">Settings</NavButton>
        </nav>
        <div className="sidebar-foot">
          <div className="local-pill"><span className="pulse-dot" />Local only</div>
          <p>Runs quietly in the tray.<br />Your clips never leave this machine.</p>
          <small>{isRecording ? 'buffer armed' : 'buffer stopped'} · v0.1</small>
        </div>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <span className="eyebrow">{view === 'library' ? 'Your moments' : view === 'capture' ? 'Capture desk' : 'Control room'}</span>
            <h1>{view === 'library' ? 'Library' : view === 'capture' ? 'Capture' : 'Settings'}</h1>
          </div>
          <div className={`engine-state ${status.state}`}><span />{isRecording ? 'Buffer armed' : 'Buffer stopped'}</div>
        </header>

        {view === 'library' && <LibraryView clips={clips} selectedClip={selectedClip} setSelectedClip={setSelectedClip} onSave={() => void saveReplay()} onOpen={(path) => void invoke('open_path', { path })} isRecording={isRecording} voiceReady={voiceReady} hotkeyLabel={hotkeyLabel} replaySeconds={settings.replaySeconds} onToggle={() => void toggleCapture()} onConfigure={() => setView('settings')} />}
        {view === 'capture' && <CaptureView isRecording={isRecording} onToggle={() => void toggleCapture()} message={message} status={status} voiceReady={voiceReady} />}
        {view === 'settings' && <SettingsView draft={draft} setDraft={setDraft} onChooseDirectory={() => void chooseOutputDirectory()} onRefreshDevices={() => void refreshDevices()} onApply={() => void applySettings()} saving={savingSettings} />}

        <footer className="status-line" aria-live="polite"><span className="status-check">✓</span>{message}<span className="status-meta">{status.platform} · {status.backend}</span></footer>
      </section>
      {flash && <div className="flash-notice" role="status" aria-live="assertive"><div className="flash-mark">✓</div><div><strong>{flash.title}</strong><span>{flash.body}</span></div></div>}
    </main>
  )
}

function LibraryView({ clips, selectedClip, setSelectedClip, onSave, onOpen, isRecording, voiceReady, hotkeyLabel, replaySeconds, onToggle, onConfigure }: { clips: Clip[]; selectedClip: Clip | null; setSelectedClip: (clip: Clip | null) => void; onSave: () => void; onOpen: (path: string) => void; isRecording: boolean; voiceReady: boolean; hotkeyLabel: string; replaySeconds: number; onToggle: () => void; onConfigure: () => void }) {
  return <div className="content-grid">
    <section className="hero-panel">
      <div className="hero-copy"><span className="eyebrow warm">Instant replay</span><h2>Keep the moment<br /><em>before it passes.</em></h2><p>{voiceReady ? 'Say “Courage, have it” at any moment. Courage saves the last replay without bringing the app forward.' : `Press save or ${hotkeyLabel}. Courage keeps the last ${replaySeconds} seconds locally.`}</p><div className="hero-actions"><button className="primary-button" type="button" onClick={onSave}><span>◆</span> Save last {replaySeconds}s <kbd>{hotkeyLabel}</kbd></button><button className="quiet-button" type="button" onClick={onConfigure}>Configure</button></div></div><div className="hero-orbit" aria-hidden="true"><div className="orbit orbit-one" /><div className="orbit orbit-two" /><div className="orbit-core">{replaySeconds}<span>s</span></div></div>
    </section>
    <section className="quick-panel"><div className="section-heading"><span>Capture state</span><span className="signal">{isRecording ? '● Armed' : '○ Stopped'}</span></div><div className="record-control"><div className={`record-ring ${isRecording ? 'active' : ''}`}><button type="button" aria-label={isRecording ? 'Stop replay buffer' : 'Start replay buffer'} onClick={onToggle}><span /></button></div></div><strong>{isRecording ? 'Replay buffer is live' : 'Replay buffer is stopped'}</strong><p>{voiceReady ? 'Listening for the phrase in the tray.' : 'Choose a microphone and enable the voice trigger in Settings.'}</p><button className="outline-button" type="button" onClick={onToggle}>{isRecording ? 'Stop buffer' : 'Start buffer'}</button></section>
    <section className="library-section"><div className="section-heading"><div><span className="eyebrow">Local library</span><h3>Recent captures</h3></div><span className="library-count">{clips.length} {clips.length === 1 ? 'clip' : 'clips'}</span></div>{clips.length === 0 ? <EmptyLibrary onSave={onSave} /> : <div className="clip-grid">{clips.map((clip) => <ClipCard key={clip.id} clip={clip} selected={selectedClip?.id === clip.id} onSelect={setSelectedClip} onOpen={onOpen} />)}</div>}</section>
    {selectedClip && <ClipInspector clip={selectedClip} onClose={() => setSelectedClip(null)} onOpen={onOpen} />}
  </div>
}

function SettingsView({ draft, setDraft, onChooseDirectory, onRefreshDevices, onApply, saving }: { draft: Settings; setDraft: (settings: Settings) => void; onChooseDirectory: () => void; onRefreshDevices: () => void; onApply: () => void; saving: boolean }) {
  return <section className="detail-view settings-view"><div className="detail-header"><span className="eyebrow warm">Control room</span><h2>Set the room<br /><em>your way.</em></h2><p>These settings are saved locally. Changes to the microphone restart the replay buffer so the next clip uses the device you chose.</p></div><div className="settings-list">
    <div className="setting-row setting-row-wide"><div className="setting-label"><strong>Recording microphone</strong><small>Audio included in every replay</small></div><div className="device-picker"><select id="audio-device" value={draft.selectedAudioDevice ?? ''} onChange={(event) => setDraft({ ...draft, selectedAudioDevice: event.target.value || null })}><option value="">No microphone</option>{draft.audioDevices.map((device) => <option value={device.id} key={device.id}>{device.name}{device.isDefault ? ' · default' : ''}</option>)}</select><button className="small-button" type="button" onClick={onRefreshDevices}>Refresh</button></div></div>
    <div className="setting-row setting-row-wide"><div className="setting-label"><strong>Clip folder</strong><small>MP4 files are written here immediately after saving</small></div><div className="path-picker"><code title={draft.outputDir}>{draft.outputDir || 'Choose a folder'}</code><button className="small-button" type="button" onClick={onChooseDirectory}>Choose folder</button></div></div>
    <div className="setting-row"><div className="setting-label"><strong>Replay length</strong><small>How far back save reaches</small></div><span className="range-control"><output>{draft.replaySeconds}s</output><input id="replay-length" aria-label="Replay length in seconds" type="range" min="5" max="120" step="5" value={draft.replaySeconds} onChange={(event) => setDraft({ ...draft, replaySeconds: Number(event.target.value) })} /></span></div>
    <div className="setting-row"><div className="setting-label"><strong>Save replay hotkey</strong><small>Works while Courage is hidden in the tray</small></div><select id="save-hotkey" className="hotkey-select" value={draft.hotkey} onChange={(event) => setDraft({ ...draft, hotkey: event.target.value })}>{hotkeyOptions.map((option) => <option value={option.value} key={option.value}>{option.label}</option>)}</select></div>
    <div className="setting-row"><div className="setting-label"><strong>Voice trigger</strong><small>“Courage, have it” saves the last replay</small></div><label className="switch-label"><input type="checkbox" checked={draft.voiceEnabled} disabled={!draft.nativeVoiceAvailable} onChange={(event) => setDraft({ ...draft, voiceEnabled: event.target.checked })} /><span className="switch" aria-hidden="true" /></label></div>
    <div className="setting-row capability-row"><div className="setting-label"><strong>Voice engine</strong><small>{draft.nativeVoiceAvailable ? 'Native Windows speech recognition can listen while the app is in the tray.' : 'Native tray voice recognition is currently implemented for Windows.'}</small></div><span className={`capability ${draft.nativeVoiceAvailable ? 'available' : 'limited'}`}><i />{draft.nativeVoiceAvailable ? 'Available' : 'Platform limited'}</span></div>
  </div><div className="settings-actions"><button className="primary-button" type="button" onClick={onApply} disabled={saving}>{saving ? 'Applying…' : 'Apply settings'}</button><span>Saved to your local Courage profile.</span></div></section>
}

function CaptureView({ isRecording, onToggle, message, status, voiceReady }: { isRecording: boolean; onToggle: () => void; message: string; status: EngineStatus; voiceReady: boolean }) {
  return <section className="detail-view"><div className="detail-header"><span className="eyebrow warm">Capture status</span><h2>{isRecording ? 'Your replay buffer is live.' : 'Start the buffer before you play.'}</h2><p>{message}</p></div><div className="capture-card"><div className={`big-record-indicator ${isRecording ? 'active' : ''}`} /><div><strong>{isRecording ? 'Capturing the desktop now' : 'Capture is stopped'}</strong><p>{isRecording ? `${voiceReady ? 'Listening for “Courage, have it” in the tray. ' : ''}Saved clips are written to ${status.outputDir}.` : 'No frames are being recorded until you start the buffer.'}</p></div><button className="primary-button" type="button" onClick={onToggle}>{isRecording ? 'Stop buffer' : 'Start buffer'}</button></div><div className="info-grid"><InfoBlock title="Video source" value={status.backend} /><InfoBlock title="Audio input" value={status.audioDevice || 'No microphone selected'} /><InfoBlock title="Clip folder" value={status.outputDir || 'Not configured'} /></div></section>
}

function EmptyLibrary({ onSave }: { onSave: () => void }) { return <div className="empty-library"><div className="empty-mark">◆</div><strong>No captures yet</strong><p>Start the buffer, play normally, then save with the hotkey or say “Courage, have it”.</p><button className="outline-button" type="button" onClick={onSave}>Save a replay</button></div> }

function ClipCard({ clip, selected, onSelect, onOpen }: { clip: Clip; selected: boolean; onSelect: (clip: Clip) => void; onOpen: (path: string) => void }) { return <article className={`clip-card ${selected ? 'selected' : ''}`}><button className="clip-thumb real" type="button" onClick={() => onSelect(clip)} aria-label={`Inspect ${clip.title}`}><span className="play-icon">↗</span><span className="clip-duration">{formatDuration(clip.durationMs)}</span></button><div className="clip-info"><strong>{clip.title}</strong><span title={clip.path}>{new Date(clip.createdAt * 1000).toLocaleString()}</span><button className="open-clip" type="button" onClick={() => onOpen(clip.path)}>Open clip ↗</button></div></article> }

function ClipInspector({ clip, onClose, onOpen }: { clip: Clip; onClose: () => void; onOpen: (path: string) => void }) { return <div className="clip-player" role="dialog" aria-labelledby="clip-inspector-title"><div className="player-heading"><div><span className="eyebrow">Selected capture</span><strong id="clip-inspector-title">{clip.title}</strong></div><button className="quiet-button" type="button" onClick={onClose}>Close</button></div><p className="clip-path">{clip.path}</p><div className="inspector-actions"><button className="primary-button" type="button" onClick={() => onOpen(clip.path)}>Open in player ↗</button><button className="outline-button" type="button" onClick={() => onOpen(clip.path.replace(/[\\/][^\\/]+$/, ''))}>Open folder</button></div></div> }

function InfoBlock({ title, value }: { title: string; value: string }) { return <div className="info-block"><span>{title}</span><strong title={value}>{value}</strong></div> }

function formatDuration(durationMs: number) { const seconds = Math.max(0, Math.round(durationMs / 1000)); return `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}` }

function NavButton({ active, onClick, icon, children }: { active: boolean; onClick: () => void; icon: string; children: string }) { return <button className={`nav-button ${active ? 'active' : ''}`} type="button" onClick={onClick}><span aria-hidden="true">{icon}</span>{children}</button> }

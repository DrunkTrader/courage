import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from 'react'
import { convertFileSrc, invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { register, unregister } from '@tauri-apps/plugin-global-shortcut'

type View = 'library' | 'capture' | 'settings'
type Theme = 'light' | 'dark' | 'rusty'

type Clip = {
  id: number
  path: string
  title: string
  durationMs: number
  createdAt: number
  thumbnailPath: string | null
  favorite: boolean
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
  fps: number
  resolutionPreset: string
  width: number | null
  height: number | null
  videoQuality: string
  videoEncoder: string
  hardwareAcceleration: string
  cursorCapture: boolean
  audioEnabled: boolean
  systemAudioEnabled: boolean
  separateMicrophoneTrack: boolean
  audioSampleRate: number
  audioChannels: number
  captureSource: string
  voiceWakePhrase: string
  voiceCommandPhrase: string
  voiceWakeOnly: boolean
  voiceLocale: string
  theme: Theme
}

type EngineStatus = {
  revision: number
  state: 'stopped' | 'starting' | 'recording' | 'stopping' | 'error' | 'unavailable'
  sessionId: string | null
  pid: number | null
  error: string | null
  voice: VoiceDiagnostics
  platform: string
  backend: string
  audioDevice: string | null
  outputDir: string
  replaySeconds: number
  fps: number
  resolution: string
  videoEncoder: string
  cursorCapture: boolean
}

type VoiceRecognitionResult = {
  generation: string
  phrase: string
  confidence: number | null
  decision: VoiceDecision
  testMode: boolean
  saved: boolean
}

type VoiceDecision = {
  normalized: string
  wakeMatched: boolean
  commandMatched: boolean
  wouldTrigger: boolean
  reason: string
}

type VoiceDiagnostics = {
  generation: string | null
  engine: string
  locale: string
  microphone: string
  audioFormat: string | null
  availability: string
  listening: boolean
  testMode: boolean
  error: string | null
}

type IconName =
  | 'library'
  | 'capture'
  | 'settings'
  | 'save'
  | 'play'
  | 'folder'
  | 'trash'
  | 'refresh'
  | 'microphone'
  | 'keyboard'
  | 'voice'
  | 'chevron'
  | 'check'
  | 'monitor'
  | 'audio'
  | 'cursor'
  | 'info'

const emptySettings: Settings = {
  outputDir: '',
  defaultOutputDir: '',
  audioDevices: [],
  selectedAudioDevice: null,
  hotkey: 'CommandOrControl+Shift+1',
  replaySeconds: 30,
  voiceEnabled: true,
  nativeVoiceAvailable: false,
  fps: 30,
  resolutionPreset: 'native',
  width: null,
  height: null,
  videoQuality: 'balanced',
  videoEncoder: 'auto',
  hardwareAcceleration: 'auto',
  cursorCapture: true,
  audioEnabled: true,
  systemAudioEnabled: false,
  separateMicrophoneTrack: false,
  audioSampleRate: 48000,
  audioChannels: 2,
  captureSource: 'desktop',
  voiceWakePhrase: 'Courage',
  voiceCommandPhrase: 'have it',
  voiceWakeOnly: false,
  voiceLocale: 'en-US',
  theme: 'dark',
}

const emptyStatus: EngineStatus = {
  revision: 0,
  state: 'unavailable',
  sessionId: null,
  pid: null,
  error: null,
  voice: { generation: null, engine: 'System.Speech · constrained grammar', locale: 'en-US', microphone: 'Windows default recording device', audioFormat: null, availability: 'unchecked', listening: false, testMode: false, error: null },
  platform: 'browser preview',
  backend: 'Desktop runtime required',
  audioDevice: null,
  outputDir: '',
  replaySeconds: 30,
  fps: 30,
  resolution: 'Native display',
  videoEncoder: 'auto',
  cursorCapture: true,
}

const captureSettingKeys: Array<keyof Settings> = [
  'selectedAudioDevice',
  'replaySeconds',
  'fps',
  'resolutionPreset',
  'width',
  'height',
  'videoQuality',
  'videoEncoder',
  'hardwareAcceleration',
  'cursorCapture',
  'audioEnabled',
  'systemAudioEnabled',
  'separateMicrophoneTrack',
  'audioSampleRate',
  'audioChannels',
  'captureSource',
]

const isDesktop = () => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

function initialTheme(): Theme {
  if (typeof window === 'undefined') return 'dark'
  const value = window.localStorage.getItem('courage-theme')
  return value === 'light' || value === 'rusty' || value === 'dark' ? value : 'dark'
}

function errorText(error: unknown) {
  const text = String(error)
  return text.startsWith('Error: ') ? text.slice(7) : text
}

export function App() {
  const [view, setView] = useState<View>('library')
  const [settings, setSettings] = useState<Settings>(emptySettings)
  const [draft, setDraft] = useState<Settings>(emptySettings)
  const [theme, setTheme] = useState<Theme>(initialTheme)
  const [status, setStatus] = useState<EngineStatus>(emptyStatus)
  const [clips, setClips] = useState<Clip[]>([])
  const [libraryError, setLibraryError] = useState<string | null>(null)
  const [lastVoiceResult, setLastVoiceResult] = useState<VoiceRecognitionResult | null>(null)
  const [selectedClip, setSelectedClip] = useState<Clip | null>(null)
  const [favoritesOnly, setFavoritesOnly] = useState(false)
  const [message, setMessage] = useState('Starting the local capture engine…')
  const [flash, setFlash] = useState<{ title: string; body: string } | null>(null)
  const [savingSettings, setSavingSettings] = useState(false)
  const [captureBusy, setCaptureBusy] = useState(false)
  const [exiting, setExiting] = useState(false)
  const [libraryBusy, setLibraryBusy] = useState(false)
  const savingReplay = useRef(false)
  const exitingRef = useRef(false)
  const lastHotkeyPress = useRef(0)
  const acceptStatus = useCallback((next: EngineStatus) => {
    if (!exitingRef.current) setStatus((current) => next.revision >= current.revision ? next : current)
  }, [])

  const showFlash = useCallback((title: string, body: string) => {
    setFlash({ title, body })
    window.setTimeout(() => setFlash(null), 4200)
  }, [])

  const fetchClips = useCallback(async () => {
    if (!isDesktop()) return []
    return invoke<Clip[]>('list_clips')
  }, [])

  const refreshClips = useCallback(async () => {
    if (!isDesktop()) return
    setLibraryBusy(true)
    try {
      const nextClips = await fetchClips()
      setClips(nextClips)
      setLibraryError(null)
      setSelectedClip((current) => current && nextClips.some((clip) => clip.id === current.id) ? current : null)
    } catch (error) {
      const detail = errorText(error)
      setClips([])
      setSelectedClip(null)
      setLibraryError(detail)
      setMessage(`Library unavailable: ${detail}`)
    } finally {
      setLibraryBusy(false)
    }
  }, [fetchClips])

  const saveReplay = useCallback(async (trigger: 'button' | 'hotkey' | 'voice' = 'button') => {
    if (exitingRef.current) return
    if (!isDesktop()) {
      setMessage('Run the Courage desktop build to capture the display.')
      return
    }
    if (savingReplay.current) return
    savingReplay.current = true
    try {
      const clip = await invoke<Clip>('save_replay')
      setClips((current) => [clip, ...current.filter((item) => item.id !== clip.id)])
      setSelectedClip(clip)
      const source = trigger === 'voice' ? 'Voice trigger' : trigger === 'hotkey' ? 'Hotkey' : 'Save action'
      setMessage(`${source} saved ${clip.title}.`)
      showFlash('Clip saved', clip.path)
    } catch (error) {
      setMessage(`Could not save replay: ${errorText(error)}`)
      showFlash('Could not save clip', errorText(error))
    } finally {
      savingReplay.current = false
    }
  }, [showFlash])

  useEffect(() => {
    document.documentElement.dataset.theme = theme
    window.localStorage.setItem('courage-theme', theme)
  }, [theme])

  useEffect(() => {
    if (!isDesktop()) {
      setMessage('Desktop runtime required for capture.')
      return
    }
    setCaptureBusy(true)
    let disposed = false
    const listeners = Promise.all([
      listen<Clip>('clip-saved', (event) => {
        if (disposed) return
        const clip = event.payload
        setClips((current) => [clip, ...current.filter((item) => item.id !== clip.id)])
        setSelectedClip(clip)
        setMessage(`Saved ${clip.title}.`)
        showFlash('Clip saved', clip.path)
      }),
      listen('library-changed', () => {
        if (!disposed) void refreshClips()
      }),
      listen<string>('library-error', (event) => {
        if (!disposed) {
          setClips([])
          setSelectedClip(null)
          setLibraryError(event.payload)
          setMessage(`Library sync needs attention: ${event.payload}`)
        }
      }),
      listen<string>('capture-error', (event) => {
        if (!disposed) {
          setMessage(event.payload)
          showFlash('Capture needs attention', event.payload)
        }
      }),
      listen<VoiceRecognitionResult>('voice-result', (event) => {
        if (!disposed) setLastVoiceResult(event.payload)
      }),
      listen<EngineStatus>('engine-status', (event) => {
        if (!disposed) acceptStatus(event.payload)
      }),
      listen('app-exiting', () => { exitingRef.current = true; if (!disposed) { setExiting(true); setMessage('Closing Courage: releasing microphone and capture resources…') } }),
      listen('app-exit-failed', () => { exitingRef.current = false; if (!disposed) setExiting(false) }),
    ])

    const boot = async () => {
      try {
        const [loadedSettings, loadedClips] = await Promise.all([
          invoke<Settings>('get_settings'),
          fetchClips(),
          listeners,
        ]).then(([nextSettings, nextClips]) => [nextSettings, nextClips] as [Settings, Clip[]])
        if (disposed) return
        setSettings(loadedSettings)
        setDraft(loadedSettings)
        setTheme(loadedSettings.theme)
        setClips(loadedClips)
        const nextStatus = await invoke<EngineStatus>('start_capture')
        if (disposed) return
        acceptStatus(nextStatus)
        setMessage(nextStatus.state === 'recording' ? 'Replay buffer armed. Courage is ready.' : 'Replay buffer is ready.')
      } catch (error) {
        if (!disposed) {
          setStatus({ ...emptyStatus, backend: errorText(error) })
          setMessage(`Courage could not start: ${errorText(error)}`)
        }
      } finally {
        if (!disposed) setCaptureBusy(false)
      }
    }
    void boot()
    const statusTimer = window.setInterval(() => {
      if (exitingRef.current) return
      void invoke<EngineStatus>('engine_status').then((nextStatus) => {
        if (!disposed) acceptStatus(nextStatus)
      }).catch(() => undefined)
    }, 2000)
    return () => {
      disposed = true
      window.clearInterval(statusTimer)
      void listeners.then((unlisteners) => unlisteners.forEach((unlisten) => unlisten())).catch(() => undefined)
    }
  }, [acceptStatus, fetchClips, refreshClips, showFlash])

  useEffect(() => {
    if (!isDesktop() || !settings.hotkey) return
    let active = true
    void register(settings.hotkey, (event) => {
      if (active && event.state === 'Pressed') {
        const now = Date.now()
        if (now - lastHotkeyPress.current < 350) return
        lastHotkeyPress.current = now
        void saveReplay('hotkey')
      }
    }).catch((error) => {
      if (active) setMessage(`Hotkey unavailable: ${errorText(error)}`)
    })
    return () => {
      active = false
      void unregister(settings.hotkey).catch(() => undefined)
    }
  }, [saveReplay, settings.hotkey])

  useEffect(() => {
    if (!isDesktop()) return
    const preventBrowserActions = (event: globalThis.KeyboardEvent) => {
      const target = event.target as HTMLElement | null
      if (target?.closest('input, textarea, select, [contenteditable="true"]')) return
      const browserAction = event.key === 'F5' || event.key === 'F12' ||
        (event.ctrlKey && ['r', 'p', 's'].includes(event.key.toLowerCase())) ||
        (event.ctrlKey && event.shiftKey && event.key.toLowerCase() === 'i')
      if (browserAction) event.preventDefault()
    }
    window.addEventListener('keydown', preventBrowserActions)
    return () => window.removeEventListener('keydown', preventBrowserActions)
  }, [])

  const isRecording = status.state === 'recording'
  const voiceReady = settings.voiceEnabled && status.voice.listening && !status.voice.testMode
  const needsRestart = useMemo(
    () => captureSettingKeys.some((key) => settings[key] !== draft[key]),
    [draft, settings],
  )

  async function toggleCapture() {
    if (captureBusy) return
    setCaptureBusy(true)
    try {
      const nextStatus = isRecording || (status.state === 'error' && status.sessionId !== null)
        ? await invoke<EngineStatus>('stop_capture')
        : await invoke<EngineStatus>('start_capture')
      acceptStatus(nextStatus)
      setMessage(nextStatus.state === 'recording' ? 'Replay buffer armed.' : 'Replay buffer stopped.')
    } catch (error) {
      setMessage(`Capture could not change state: ${errorText(error)}`)
    } finally {
      setCaptureBusy(false)
    }
  }

  async function refreshDevices() {
    try {
      const audioDevices = await invoke<AudioDevice[]>('list_audio_devices')
      setDraft((current) => ({ ...current, audioDevices }))
      setMessage(audioDevices.length ? `${audioDevices.length} recording device${audioDevices.length === 1 ? '' : 's'} found.` : 'No recording devices found. Video-only capture remains available.')
    } catch (error) {
      setMessage(`Microphone discovery failed: ${errorText(error)}`)
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
    const wasRecording = isRecording
    const restart = wasRecording && needsRestart
    try {
      if (isDesktop() && draft.hotkey !== settings.hotkey) {
        await register(draft.hotkey, () => undefined)
        await unregister(draft.hotkey)
      }
      const nextSettings = await invoke<Settings>('save_settings', {
        settings: {
          outputDir: draft.outputDir,
          selectedAudioDevice: draft.selectedAudioDevice,
          hotkey: draft.hotkey,
          replaySeconds: draft.replaySeconds,
          voiceEnabled: draft.voiceEnabled,
          fps: draft.fps,
          resolutionPreset: draft.resolutionPreset,
          width: draft.width,
          height: draft.height,
          videoQuality: draft.videoQuality,
          videoEncoder: draft.videoEncoder,
          hardwareAcceleration: draft.hardwareAcceleration,
          cursorCapture: draft.cursorCapture,
          audioEnabled: draft.audioEnabled,
          systemAudioEnabled: draft.systemAudioEnabled,
          separateMicrophoneTrack: draft.separateMicrophoneTrack,
          audioSampleRate: draft.audioSampleRate,
          audioChannels: draft.audioChannels,
          captureSource: draft.captureSource,
          voiceWakePhrase: draft.voiceWakePhrase,
          voiceCommandPhrase: draft.voiceCommandPhrase,
          voiceWakeOnly: draft.voiceWakeOnly,
          voiceLocale: draft.voiceLocale,
          theme: draft.theme,
        },
      })
      setSettings(nextSettings)
      setDraft(nextSettings)
      setTheme(nextSettings.theme)
      let nextStatus = status
      if (restart) {
        nextStatus = await invoke<EngineStatus>('stop_capture')
        acceptStatus(nextStatus)
        nextStatus = await invoke<EngineStatus>('start_capture')
      }
      acceptStatus(nextStatus)
      await refreshClips()
      setMessage(restart ? 'Settings saved and the replay buffer restarted safely.' : 'Settings saved locally.')
    } catch (error) {
      setMessage(`Settings or buffer update needs attention: ${errorText(error)}`)
    } finally {
      setSavingSettings(false)
    }
  }

  function resetSettings() {
    const next = {
      ...emptySettings,
      outputDir: draft.defaultOutputDir,
      defaultOutputDir: draft.defaultOutputDir,
      audioDevices: draft.audioDevices,
      nativeVoiceAvailable: draft.nativeVoiceAvailable,
    }
    setDraft(next)
    setTheme(next.theme)
    setMessage('Defaults loaded. Apply settings to save them.')
  }

  async function openPath(path: string) {
    try {
      await invoke('open_path', { path })
    } catch (error) {
      setMessage(`Could not open path: ${errorText(error)}`)
    }
  }

  async function copyPath(path: string) {
    try {
      await navigator.clipboard.writeText(path)
      setMessage('Clip path copied to the clipboard.')
    } catch (error) {
      setMessage(`Could not copy clip path: ${errorText(error)}`)
    }
  }

  async function toggleFavorite(clip: Clip) {
    const favorite = !clip.favorite
    try {
      await invoke('set_clip_favorite', { id: clip.id, favorite })
      setClips((current) => current.map((item) => item.id === clip.id ? { ...item, favorite } : item))
      setSelectedClip((current) => current?.id === clip.id ? { ...current, favorite } : current)
    } catch (error) {
      setMessage(`Could not update favorite: ${errorText(error)}`)
    }
  }

  function handleTrimmed(clip: Clip) {
    setClips((current) => [clip, ...current.filter((item) => item.id !== clip.id)])
    setSelectedClip(clip)
    setMessage(`Trimmed ${clip.title}.`)
    showFlash('Trim saved', clip.path)
  }

  async function deleteClip(clip: Clip) {
    if (!window.confirm(`Delete ${clip.title} from this computer?`)) return
    try {
      await invoke('delete_clip', { id: clip.id })
      setClips((current) => current.filter((item) => item.id !== clip.id))
      if (selectedClip?.id === clip.id) setSelectedClip(null)
      setMessage(`${clip.title} deleted.`)
    } catch (error) {
      setMessage(`Could not delete clip: ${errorText(error)}`)
    }
  }

  function suppressBrowserContextMenu(event: MouseEvent<HTMLElement>) {
    const target = event.target as HTMLElement | null
    if (target?.closest('input, textarea, select, [contenteditable="true"]')) return
    event.preventDefault()
  }

  return (
    <main className="app-shell" onContextMenu={suppressBrowserContextMenu}>
      <aside className="sidebar">
        <div className="brand-lockup">
          <div className="brand-mark" aria-hidden="true">C</div>
          <div><strong>Courage</strong><span>have it.</span></div>
        </div>
        <nav className="primary-nav" aria-label="Primary navigation">
          <NavButton active={view === 'library'} onClick={() => setView('library')} icon="library">Library</NavButton>
          <NavButton active={view === 'capture'} onClick={() => setView('capture')} icon="capture">Capture</NavButton>
          <NavButton active={view === 'settings'} onClick={() => setView('settings')} icon="settings">Settings</NavButton>
        </nav>
        <div className="sidebar-foot">
          <div className="local-pill"><span className="pulse-dot" />Local only</div>
          <p>Close quits Courage.<br />Hide keeps recording in the tray.</p>
          <button className="quiet-button" type="button" disabled={exiting} onClick={() => void invoke('hide_to_tray')}>Hide to tray</button>
          <small>{isRecording ? 'buffer armed' : 'buffer stopped'} · v0.1</small>
        </div>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <span className="eyebrow">{view === 'library' ? 'Your moments' : view === 'capture' ? 'Capture desk' : 'Control room'}</span>
            <h1>{view === 'library' ? 'Library' : view === 'capture' ? 'Capture' : 'Settings'}</h1>
          </div>
          <div className={`engine-state ${isRecording ? 'capturing' : status.state}`}><span />{exiting ? 'Shutting down' : isRecording ? 'Buffer armed' : `Buffer ${status.state}`}</div>
        </header>

        {view === 'library' && (
          <LibraryView
            clips={clips}
            selectedClip={selectedClip}
            onSelect={setSelectedClip}
            onSave={() => void saveReplay()}
            onOpen={(path) => void openPath(path)}
            onReveal={(path) => void openPath(path.replace(/[\\/][^\\/]+$/, ''))}
            onCopyPath={(path) => void copyPath(path)}
            onDelete={(clip) => void deleteClip(clip)}
            onFavorite={(clip) => void toggleFavorite(clip)}
            onTrimmed={handleTrimmed}
            favoritesOnly={favoritesOnly}
            onFavoritesOnlyChange={setFavoritesOnly}
            onRefresh={() => void refreshClips()}
            isRecording={isRecording}
            voiceReady={voiceReady}
            replaySeconds={settings.replaySeconds}
            hotkey={settings.hotkey}
            wakePhrase={settings.voiceWakePhrase}
            commandPhrase={settings.voiceWakeOnly ? '' : settings.voiceCommandPhrase}
            onToggle={() => void toggleCapture()}
            onConfigure={() => setView('settings')}
            busy={libraryBusy}
            error={libraryError}
            captureBusy={captureBusy || exiting}
          />
        )}
         {view === 'capture' && <CaptureView isRecording={isRecording} onToggle={() => void toggleCapture()} message={message} status={status} voiceReady={voiceReady} busy={captureBusy || exiting} />}
        {view === 'settings' && (
          <SettingsView
            draft={draft}
            setDraft={setDraft}
            theme={theme}
            onThemeChange={(nextTheme) => {
              setTheme(nextTheme)
              setDraft((current) => ({ ...current, theme: nextTheme }))
            }}
            onChooseDirectory={() => void chooseOutputDirectory()}
            onRefreshDevices={() => void refreshDevices()}
            onApply={() => void applySettings()}
            onReset={resetSettings}
            saving={savingSettings || captureBusy || exiting}
            needsRestart={needsRestart}
            message={message}
            lastVoiceResult={lastVoiceResult}
            voice={status.voice}
            platform={status.platform}
          />
        )}

        <footer className="status-line" aria-live="polite"><span className="status-check"><Icon name="check" size={12} /></span>{message}<span className="status-meta">{status.platform} · {status.backend}</span></footer>
      </section>
      {flash && <div className="flash-notice" role="status" aria-live="assertive"><div className="flash-mark"><Icon name="check" size={13} /></div><div><strong>{flash.title}</strong><span>{flash.body}</span></div></div>}
    </main>
  )
}

type LibraryProps = {
  clips: Clip[]
  selectedClip: Clip | null
  onSelect: (clip: Clip | null) => void
  onSave: () => void
  onOpen: (path: string) => void
  onReveal: (path: string) => void
  onCopyPath: (path: string) => void
  onDelete: (clip: Clip) => void
  onFavorite: (clip: Clip) => void
  onTrimmed: (clip: Clip) => void
  favoritesOnly: boolean
  onFavoritesOnlyChange: (value: boolean) => void
  onRefresh: () => void
  isRecording: boolean
  voiceReady: boolean
  replaySeconds: number
  hotkey: string
  wakePhrase: string
  commandPhrase: string
  onToggle: () => void
  onConfigure: () => void
  busy: boolean
  error: string | null
  captureBusy: boolean
}

function LibraryView({ clips, selectedClip, onSelect, onSave, onOpen, onReveal, onCopyPath, onDelete, onFavorite, onTrimmed, favoritesOnly, onFavoritesOnlyChange, onRefresh, isRecording, voiceReady, replaySeconds, hotkey, wakePhrase, commandPhrase, onToggle, onConfigure, busy, error, captureBusy }: LibraryProps) {
  const visibleClips = favoritesOnly ? clips.filter((clip) => clip.favorite) : clips
  return (
    <div className="content-grid">
      <section className="hero-panel">
        <div className="hero-copy"><span className="eyebrow warm">Instant replay</span><h2>Save what just<br /><em>happened.</em></h2><p>{voiceReady ? `Say “${wakePhrase}, ${commandPhrase}” to save without bringing Courage forward.` : `Courage keeps the last ${replaySeconds} seconds local and ready.`}</p><div className="hero-actions"><button className="primary-button" type="button" onClick={onSave}><Icon name="save" size={15} />Save last {replaySeconds}s <kbd>{prettyShortcut(hotkey)}</kbd></button><button className="quiet-button" type="button" onClick={onConfigure}>Configure <Icon name="chevron" size={13} /></button></div></div><div className="hero-orbit" aria-hidden="true"><div className="orbit orbit-one" /><div className="orbit orbit-two" /><div className="orbit-core">{replaySeconds}<span>s</span></div></div>
      </section>
      <section className="quick-panel"><div className="section-heading"><span>Capture state</span><span className={`signal ${isRecording ? 'on' : ''}`}><span />{captureBusy ? 'Changing' : isRecording ? 'Armed' : 'Stopped'}</span></div><div className="record-control"><div className={`record-ring ${isRecording ? 'active' : ''}`}><button type="button" disabled={captureBusy} aria-label={isRecording ? 'Stop replay buffer' : 'Start replay buffer'} onClick={onToggle}><span /></button></div></div><strong>{captureBusy ? 'Changing capture state…' : isRecording ? 'Replay buffer is live' : 'Replay buffer is stopped'}</strong><p>{voiceReady ? 'Listening for the configured phrase in the tray.' : 'Start the buffer before saving a replay.'}</p><button className="outline-button" type="button" disabled={captureBusy} onClick={onToggle}>{captureBusy ? 'Working…' : isRecording ? 'Stop buffer' : 'Start buffer'}</button></section>
       <section className="library-section"><div className="section-heading library-heading"><div><span className="eyebrow">Local library</span><h3>Recent captures</h3></div><div className="library-tools"><span className="library-count">{visibleClips.length} {visibleClips.length === 1 ? 'clip' : 'clips'}</span><button className={`icon-button ${favoritesOnly ? 'selected' : ''}`} type="button" onClick={() => onFavoritesOnlyChange(!favoritesOnly)} aria-pressed={favoritesOnly} aria-label={favoritesOnly ? 'Show all clips' : 'Show favorite clips'} title={favoritesOnly ? 'Show all clips' : 'Show favorite clips'}>★</button><button className="icon-button" type="button" onClick={onRefresh} disabled={busy} aria-label="Refresh library" title="Refresh library"><Icon name="refresh" size={15} /></button></div></div>{error ? <div className="state-panel error-state"><Icon name="info" size={20} /><strong>Clip folder unavailable</strong><p>{error}</p><button className="outline-button" type="button" onClick={onRefresh}><Icon name="refresh" size={14} />Try again</button></div> : busy && clips.length === 0 ? <div className="state-panel"><Icon name="refresh" size={20} /><strong>Checking the clip folder…</strong></div> : visibleClips.length === 0 && clips.length === 0 ? <EmptyLibrary onSave={onSave} /> : visibleClips.length === 0 ? <div className="state-panel"><strong>No favorite clips yet</strong><button className="outline-button" type="button" onClick={() => onFavoritesOnlyChange(false)}>Show all clips</button></div> : <div className="clip-grid">{visibleClips.map((clip) => <ClipCard key={clip.id} clip={clip} selected={selectedClip?.id === clip.id} onSelect={onSelect} onOpen={onOpen} onReveal={onReveal} onCopyPath={onCopyPath} onDelete={onDelete} onFavorite={onFavorite} />)}</div>}</section>
       {selectedClip && <ClipInspector clip={selectedClip} onClose={() => onSelect(null)} onOpen={onOpen} onReveal={onReveal} onCopyPath={onCopyPath} onDelete={onDelete} onTrimmed={onTrimmed} />}
    </div>
  )
}

type SettingsViewProps = {
  draft: Settings
  setDraft: (settings: Settings) => void
  theme: Theme
  onThemeChange: (theme: Theme) => void
  onChooseDirectory: () => void
  onRefreshDevices: () => void
  onApply: () => void
  onReset: () => void
  saving: boolean
  needsRestart: boolean
  message: string
  lastVoiceResult: VoiceRecognitionResult | null
  voice: VoiceDiagnostics
  platform: string
}

function SettingsView({ draft, setDraft, theme, onThemeChange, onChooseDirectory, onRefreshDevices, onApply, onReset, saving, needsRestart, message, lastVoiceResult, voice, platform }: SettingsViewProps) {
  const update = <K extends keyof Settings>(key: K, value: Settings[K]) => setDraft({ ...draft, [key]: value })
  const [voiceTest, setVoiceTest] = useState('Courage, have it')
  const [voiceResult, setVoiceResult] = useState<string | null>(null)
  const [testingMic, setTestingMic] = useState(false)
  const voiceConfig = { wakePhrase: draft.voiceWakePhrase, commandPhrase: draft.voiceCommandPhrase, wakeOnly: draft.voiceWakeOnly, locale: draft.voiceLocale }
  const observation = lastVoiceResult?.generation === voice.generation ? lastVoiceResult : null

  useEffect(() => {
    setVoiceResult(null)
    return () => { if (isDesktop()) void invoke('stop_voice_test').catch(() => undefined) }
  }, [draft.voiceWakePhrase, draft.voiceCommandPhrase, draft.voiceWakeOnly, draft.voiceLocale])

  async function testMicrophone() {
    setTestingMic(true)
    try {
      if (voice.testMode && voice.listening) await invoke('stop_voice_test')
      else await invoke('start_voice_test', { config: voiceConfig })
    } catch (error) {
      setVoiceResult(`Microphone test failed: ${errorText(error)}`)
    } finally { setTestingMic(false) }
  }

  async function testVoice() {
    if (!isDesktop()) {
      setVoiceResult('Run the desktop build to test native recognition.')
      return
    }
    try {
      const result = await invoke<VoiceDecision>('test_voice_phrase', { phrase: voiceTest, config: voiceConfig })
      setVoiceResult(`Text simulation only · ${result.wouldTrigger ? 'matched' : 'rejected'} · wake ${result.wakeMatched ? 'matched' : 'missed'} · command ${result.commandMatched ? 'matched' : 'missed'} · ${result.reason}`)
    } catch (error) {
      setVoiceResult(`Test failed: ${errorText(error)}`)
    }
  }

  return (
    <section className="detail-view settings-view">
      <div className="detail-header"><span className="eyebrow warm">Control room</span><h2>Make the buffer<br /><em>fit your play.</em></h2><p>Settings are stored locally. Changes marked with a restart badge are applied to the active buffer safely.</p></div>

      <SettingsSection eyebrow="Video capture" title="Picture and performance" description="Only controls connected to the FFmpeg capture command are shown.">
        <div className="field-grid">
          <SettingField label="Capture source" help="The current backend captures the full desktop."><select id="capture-source" value={draft.captureSource} onChange={(event) => update('captureSource', event.target.value)}><option value="desktop">Full desktop</option></select></SettingField>
          <SettingField label="Frame rate" help="Higher values use more CPU and storage."><select id="capture-fps" value={draft.fps} onChange={(event) => update('fps', Number(event.target.value))}><option value="15">15 FPS</option><option value="24">24 FPS</option><option value="30">30 FPS</option><option value="60">60 FPS</option><option value="120">120 FPS</option></select></SettingField>
          <SettingField label="Resolution" help="Native uses the display's current size."><select id="capture-resolution" value={draft.resolutionPreset} onChange={(event) => update('resolutionPreset', event.target.value)}><option value="native">Native display</option><option value="1920x1080">1920 × 1080</option><option value="1600x900">1600 × 900</option><option value="1280x720">1280 × 720</option><option value="custom">Custom</option></select></SettingField>
          {draft.resolutionPreset === 'custom' && <div className="inline-fields"><SettingField label="Width" help="320–7680 pixels."><input id="capture-width" type="number" min="320" max="7680" value={draft.width ?? ''} onChange={(event) => update('width', event.target.value ? Number(event.target.value) : null)} /></SettingField><SettingField label="Height" help="240–4320 pixels."><input id="capture-height" type="number" min="240" max="4320" value={draft.height ?? ''} onChange={(event) => update('height', event.target.value ? Number(event.target.value) : null)} /></SettingField></div>}
          <SettingField label="Quality preset" help="Balances encoder work and image quality."><select id="video-quality" value={draft.videoQuality} onChange={(event) => update('videoQuality', event.target.value)}><option value="performance">Performance</option><option value="balanced">Balanced</option><option value="quality">Quality</option></select></SettingField>
           <SettingField label="Video encoder" help="Automatic mode falls back safely. An explicit encoder must be available and usable or Courage reports the probe error."><select id="video-encoder" value={draft.videoEncoder} onChange={(event) => update('videoEncoder', event.target.value)}><option value="auto">Automatic fallback</option><option value="libx264">H.264 software</option><option value="h264_nvenc">NVIDIA NVENC</option><option value="h264_amf">AMD AMF</option><option value="h264_qsv">Intel Quick Sync</option></select></SettingField>
           <SettingField label="Hardware preference" help="Used only when the encoder is automatic; an explicit hardware preference is verified before capture starts."><select id="hardware-acceleration" value={draft.hardwareAcceleration} onChange={(event) => update('hardwareAcceleration', event.target.value)}><option value="auto">Automatic</option><option value="libx264">Software</option><option value="h264_nvenc">NVIDIA</option><option value="h264_amf">AMD</option><option value="h264_qsv">Intel</option></select></SettingField>
          <SettingField label="Replay length" help="How far back a save reaches."><div className="range-control"><output htmlFor="replay-length">{draft.replaySeconds}s</output><input id="replay-length" aria-label="Replay length in seconds" type="range" min="5" max="120" step="5" value={draft.replaySeconds} onChange={(event) => update('replaySeconds', Number(event.target.value))} /></div></SettingField>
        </div>
        <ToggleRow label="Capture cursor" help="Draw the pointer into the desktop frames." checked={draft.cursorCapture} onChange={(checked) => update('cursorCapture', checked)} />
      </SettingsSection>

       <SettingsSection eyebrow="Audio input" title="Sound that belongs with the moment" description="The selected device is passed to FFmpeg. Native Windows speech uses the OS default input device.">
           <ToggleRow label="Record microphone audio" help="Disable this for video-only capture." checked={draft.audioEnabled} onChange={(checked) => update('audioEnabled', checked)} />
           <ToggleRow label="Capture system audio" help={platform === 'macos' ? 'System audio capture is not connected on macOS yet.' : 'Capture the default playback device alongside the microphone.'} checked={draft.systemAudioEnabled} disabled={platform === 'macos'} onChange={(checked) => update('systemAudioEnabled', checked)} />
           <ToggleRow label="Keep a separate microphone track" help="Adds the microphone as a second track when system audio is also captured." checked={draft.separateMicrophoneTrack} disabled={!draft.audioEnabled || !draft.systemAudioEnabled} onChange={(checked) => update('separateMicrophoneTrack', checked)} />
        <div className="field-grid compact-grid">
          <SettingField label="Recording microphone" help={draft.audioDevices.length ? 'Used for clip audio.' : 'No devices were discovered.'}><div className="input-with-action"><select id="audio-device" value={draft.selectedAudioDevice ?? ''} disabled={!draft.audioEnabled} onChange={(event) => update('selectedAudioDevice', event.target.value || null)}><option value="">No microphone</option>{draft.audioDevices.map((device) => <option value={device.id} key={device.id}>{device.name}{device.isDefault ? ' · default' : ''}</option>)}</select><button className="small-button" type="button" onClick={onRefreshDevices}><Icon name="refresh" size={13} />Refresh</button></div></SettingField>
          <SettingField label="Sample rate" help="Resampled for stable A/V sync."><select id="audio-sample-rate" disabled={!draft.audioEnabled} value={draft.audioSampleRate} onChange={(event) => update('audioSampleRate', Number(event.target.value))}><option value="44100">44.1 kHz</option><option value="48000">48 kHz</option></select></SettingField>
          <SettingField label="Channels" help="Mono uses less space; stereo preserves stereo input."><select id="audio-channels" disabled={!draft.audioEnabled} value={draft.audioChannels} onChange={(event) => update('audioChannels', Number(event.target.value))}><option value="1">Mono</option><option value="2">Stereo</option></select></SettingField>
        </div>
      </SettingsSection>

      <SettingsSection eyebrow="Shortcuts" title="Save without reaching for the window" description="Global shortcuts stay active while Courage is minimized or hidden in the tray.">
        <div className="setting-row"><div className="setting-label"><strong>Save replay hotkey</strong><small>Record a supported key combination. Physical Numpad keys are preserved as Numpad1, NumpadEnter, and similar.</small></div><HotkeyRecorder value={draft.hotkey} onChange={(value) => update('hotkey', value)} /></div>
      </SettingsSection>

      <SettingsSection eyebrow="Voice trigger" title="Say it your way" description="The default requires both phrases: wake phrase, then command phrase. Wake-only is an explicit separate mode. Low-confidence Windows results are ignored.">
        <ToggleRow label="Enable native voice trigger" help={draft.nativeVoiceAvailable ? 'Windows Speech listens while the buffer is armed.' : 'Native tray recognition is available on Windows only.'} checked={draft.voiceEnabled} disabled={!draft.nativeVoiceAvailable} onChange={(checked) => update('voiceEnabled', checked)} />
        <div className="field-grid compact-grid">
          <SettingField label="Wake phrase" help="Example: Courage"><input id="voice-wake" autoComplete="off" spellCheck={false} maxLength={80} value={draft.voiceWakePhrase} onChange={(event) => update('voiceWakePhrase', event.target.value)} /></SettingField>
          <SettingField label="Command phrase" help="Required in combined mode."><input id="voice-command" autoComplete="off" spellCheck={false} maxLength={80} value={draft.voiceCommandPhrase} onChange={(event) => update('voiceCommandPhrase', event.target.value)} /></SettingField>
          <SettingField label="Recognition locale" help="Must match an installed Windows System.Speech recognizer, for example en-US or en-GB."><input id="voice-locale" autoComplete="off" spellCheck={false} maxLength={32} value={draft.voiceLocale} onChange={(event) => update('voiceLocale', event.target.value)} /></SettingField>
        </div>
        <ToggleRow label="Wake phrase alone saves" help="Off means both wake and command are required." checked={draft.voiceWakeOnly} disabled={!draft.voiceEnabled} onChange={(checked) => update('voiceWakeOnly', checked)} />
        <div className="voice-test">
          <div className="setting-label"><strong>Test the current draft</strong><small>Text simulation does not test microphone accuracy. Microphone test replaces the active voice trigger for up to 30 seconds and cannot save clips. Apply settings to resume the saved trigger. Audio and transcripts are not written to disk.</small></div>
          <div className="input-with-action"><input aria-label="Voice phrase to test" autoComplete="off" value={voiceTest} onChange={(event) => setVoiceTest(event.target.value)} /><button className="small-button" type="button" onClick={() => void testVoice()}>Test text</button></div>
          <button className="small-button" type="button" disabled={testingMic || saving || !draft.nativeVoiceAvailable} onClick={() => void testMicrophone()}><Icon name="microphone" size={13} />{testingMic ? 'Updating microphone…' : voice.testMode && voice.listening ? 'Stop microphone test' : 'Test microphone (no saves)'}</button>
          {voiceResult && <output className="test-result" aria-live="polite">{voiceResult}</output>}
          <dl><dt>Engine</dt><dd>{voice.engine}</dd><dt>Locale</dt><dd>{voice.locale}</dd><dt>Speech microphone</dt><dd>{voice.microphone}</dd><dt>Input format</dt><dd>{voice.audioFormat || 'Not reported yet'}</dd><dt>Availability / listening</dt><dd>{voice.availability} / {voice.listening ? voice.testMode ? 'testing draft · no saves' : 'listening · saved grammar' : 'stopped'}</dd></dl>
          {voice.error && <p role="alert">{voice.error}</p>}
          <div className="voice-observation" aria-live="polite"><span>Last final result</span><strong>{observation?.phrase || 'No recognition result for this session'}</strong>{observation && <small>{observation.confidence === null ? 'Confidence unavailable' : `${Math.round(observation.confidence * 100)}% engine confidence (not an accuracy percentage)`} · wake {observation.decision.wakeMatched ? 'matched' : 'missed'} · command {observation.decision.commandMatched ? 'matched' : 'missed'} · {observation.saved ? 'clip saved' : 'no clip saved'} · {observation.decision.reason}</small>}</div>
        </div>
      </SettingsSection>

      <SettingsSection eyebrow="Storage" title="Keep clips close" description="Courage reconciles this folder with its SQLite library whenever status is checked and after external changes.">
        <div className="setting-row setting-row-wide"><div className="setting-label"><strong>Clip folder</strong><small>MP4 files are written here after a save.</small></div><div className="path-picker"><code title={draft.outputDir}>{draft.outputDir || 'Choose a folder'}</code><button className="small-button" type="button" onClick={onChooseDirectory}><Icon name="folder" size={13} />Choose folder</button></div></div>
      </SettingsSection>

      <SettingsSection eyebrow="Appearance" title="Choose the room's temperature" description="Exactly three themes, persisted locally and applied immediately.">
        <div className="theme-grid">{(['light', 'dark', 'rusty'] as Theme[]).map((option) => <ThemeCard key={option} theme={option} selected={theme === option} onSelect={onThemeChange} />)}</div>
      </SettingsSection>

      <div className="settings-actions"><button className="primary-button" type="button" onClick={onApply} disabled={saving}>{saving ? 'Applying…' : 'Apply settings'}</button><button className="quiet-button" type="button" onClick={onReset}>Reset defaults</button>{needsRestart && <span className="restart-note"><Icon name="info" size={13} />Restart required for active capture</span>}<span className="settings-message">{message}</span></div>
    </section>
  )
}

function SettingsSection({ eyebrow, title, description, children }: { eyebrow: string; title: string; description: string; children: ReactNode }) {
  return <section className="settings-section"><div className="section-intro"><span className="eyebrow warm">{eyebrow}</span><h3>{title}</h3><p>{description}</p></div>{children}</section>
}

function SettingField({ label, help, children }: { label: string; help: string; children: ReactNode }) {
  return <label className="setting-field"><span>{label}</span>{children}<small>{help}</small></label>
}

function ToggleRow({ label, help, checked, disabled = false, onChange }: { label: string; help: string; checked: boolean; disabled?: boolean; onChange: (checked: boolean) => void }) {
  return <label className={`toggle-row ${disabled ? 'disabled' : ''}`}><span className="setting-label"><strong>{label}</strong><small>{help}</small></span><span className="switch-label"><input type="checkbox" checked={checked} disabled={disabled} onChange={(event) => onChange(event.target.checked)} /><span className="switch" aria-hidden="true" /></span></label>
}

function HotkeyRecorder({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  const [recording, setRecording] = useState(false)
  function captureKey(event: KeyboardEvent<HTMLInputElement>) {
    if (!recording) return
    event.preventDefault()
    if (['Control', 'Shift', 'Alt', 'Meta'].includes(event.key)) return
    const key = shortcutKey(event.code, event.key)
    if (!key) return
    const modifiers = [event.ctrlKey || event.metaKey ? 'CommandOrControl' : '', event.altKey ? 'Alt' : '', event.shiftKey ? 'Shift' : ''].filter(Boolean)
    onChange([...modifiers, key].join('+'))
    setRecording(false)
  }
  return <div className="hotkey-recorder"><input aria-label="Global save hotkey" readOnly value={recording ? 'Press a key combination…' : prettyShortcut(value)} onKeyDown={captureKey} onFocus={() => setRecording(true)} /><button className="small-button" type="button" onClick={() => setRecording((current) => !current)}><Icon name="keyboard" size={13} />{recording ? 'Cancel' : 'Record'}</button></div>
}

function shortcutKey(code: string, key: string) {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3)
  if (/^Digit\d$/.test(code)) return code.slice(5)
  if (/^Numpad\d$/.test(code)) return code
  if (/^F\d{1,2}$/.test(code)) return code
  const aliases: Record<string, string> = { Space: 'Space', Enter: 'Enter', NumpadEnter: 'NumpadEnter', NumpadAdd: 'NumpadAdd', NumpadSubtract: 'NumpadSubtract', NumpadMultiply: 'NumpadMultiply', NumpadDivide: 'NumpadDivide', NumpadDecimal: 'NumpadDecimal' }
  return aliases[code] ?? (key.length === 1 ? key.toUpperCase() : '')
}

function prettyShortcut(value: string) {
  return value.replace('CommandOrControl', 'Ctrl / Cmd').replaceAll('+', '  +  ').replaceAll('Numpad', 'Num ')
}

function ThemeCard({ theme, selected, onSelect }: { theme: Theme; selected: boolean; onSelect: (theme: Theme) => void }) {
  return <button className={`theme-card theme-${theme} ${selected ? 'selected' : ''}`} type="button" onClick={() => onSelect(theme)} aria-pressed={selected}><span className="theme-swatch"><i /><i /><i /></span><span><strong>{theme[0].toUpperCase() + theme.slice(1)}</strong><small>{theme === 'light' ? 'Clear and quiet' : theme === 'dark' ? 'Layered charcoal' : 'Warm copper'}</small></span>{selected && <Icon name="check" size={14} />}</button>
}

function CaptureView({ isRecording, onToggle, message, status, voiceReady, busy }: { isRecording: boolean; onToggle: () => void; message: string; status: EngineStatus; voiceReady: boolean; busy: boolean }) {
  return <section className="detail-view"><div className="detail-header"><span className="eyebrow warm">Capture status</span><h2>{busy ? 'Changing the buffer.' : isRecording ? 'The buffer is live.' : 'Arm the buffer before you play.'}</h2><p>{message}</p></div><div className="capture-card"><div className={`big-record-indicator ${isRecording ? 'active' : ''}`} /><div><strong>{busy ? 'Waiting for FFmpeg to finish changing state' : isRecording ? 'Capturing the desktop now' : 'Capture is stopped'}</strong><p>{isRecording ? `${voiceReady ? 'Listening for the configured voice phrase. ' : ''}Saved clips go to ${status.outputDir}.` : 'No frames are recorded until you start the buffer.'}</p></div><button className="primary-button" type="button" disabled={busy} onClick={onToggle}>{busy ? 'Working…' : isRecording ? 'Stop buffer' : 'Start buffer'}</button></div><div className="info-grid"><InfoBlock title="Video source" value={`${status.resolution} · ${status.fps} FPS`} icon="monitor" /><InfoBlock title="Encoder" value={status.videoEncoder} icon="capture" /><InfoBlock title="Audio input" value={status.audioDevice || 'Video only'} icon="audio" /><InfoBlock title="Cursor" value={status.cursorCapture ? 'Included' : 'Hidden'} icon="cursor" /><InfoBlock title="Replay length" value={`${status.replaySeconds}s`} icon="save" /><InfoBlock title="Output folder" value={status.outputDir || 'Not configured'} icon="folder" /></div></section>
}

function EmptyLibrary({ onSave }: { onSave: () => void }) { return <div className="empty-library"><div className="empty-mark"><Icon name="library" size={18} /></div><strong>No captures yet</strong><p>Start the buffer, play normally, then save with the hotkey or configured voice phrase.</p><button className="outline-button" type="button" onClick={onSave}><Icon name="save" size={14} />Save a replay</button></div> }

function ClipCard({ clip, selected, onSelect, onOpen, onReveal, onCopyPath, onDelete, onFavorite }: { clip: Clip; selected: boolean; onSelect: (clip: Clip) => void; onOpen: (path: string) => void; onReveal: (path: string) => void; onCopyPath: (path: string) => void; onDelete: (clip: Clip) => void; onFavorite: (clip: Clip) => void }) {
  const thumbnail = clip.thumbnailPath && isDesktop() ? convertFileSrc(clip.thumbnailPath) : undefined
  return <article className={`clip-card ${selected ? 'selected' : ''}`}><button className="clip-thumb real" type="button" onClick={() => onSelect(clip)} aria-label={`Inspect ${clip.title}`}>{thumbnail ? <img src={thumbnail} alt="" aria-hidden="true" /> : <span className="thumb-grid" aria-hidden="true" />}<span className="play-icon"><Icon name="play" size={15} /></span><span className="clip-duration">{formatDuration(clip.durationMs)}</span></button><div className="clip-info"><strong title={clip.title}>{clip.title}</strong><span title={clip.path}>{new Date(clip.createdAt * 1000).toLocaleString()}</span><div className="clip-actions"><button className="open-clip" type="button" onClick={() => onOpen(clip.path)}><Icon name="play" size={12} />Open</button><button className="open-clip" type="button" onClick={() => onCopyPath(clip.path)}>Copy path</button><button className="open-clip" type="button" onClick={() => onReveal(clip.path)}><Icon name="folder" size={12} />Folder</button><button className="favorite-button" type="button" aria-label={clip.favorite ? `Unfavorite ${clip.title}` : `Favorite ${clip.title}`} aria-pressed={clip.favorite} onClick={() => onFavorite(clip)}>★</button><button className="danger-button" type="button" onClick={() => onDelete(clip)} aria-label={`Delete ${clip.title}`}><Icon name="trash" size={13} /></button></div></div></article>
}

function ClipInspector({ clip, onClose, onOpen, onReveal, onCopyPath, onDelete, onTrimmed }: { clip: Clip; onClose: () => void; onOpen: (path: string) => void; onReveal: (path: string) => void; onCopyPath: (path: string) => void; onDelete: (clip: Clip) => void; onTrimmed: (clip: Clip) => void }) {
  const [start, setStart] = useState(0)
  const [end, setEnd] = useState(Math.max(0.1, clip.durationMs / 1000))
  const [trimming, setTrimming] = useState(false)
  const [trimError, setTrimError] = useState<string | null>(null)
  const source = isDesktop() ? convertFileSrc(clip.path) : undefined

  useEffect(() => {
    setStart(0)
    setEnd(Math.max(0.1, clip.durationMs / 1000))
    setTrimError(null)
  }, [clip.id, clip.durationMs])

  async function trimClip() {
    if (start < 0 || end <= start) {
      setTrimError('End must be greater than start.')
      return
    }
    setTrimming(true)
    setTrimError(null)
    try {
      const trimmed = await invoke<Clip>('trim_clip', { id: clip.id, startSeconds: start, endSeconds: end })
      onTrimmed(trimmed)
    } catch (error) {
      setTrimError(errorText(error))
    } finally {
      setTrimming(false)
    }
  }

  return <div className="clip-player" role="dialog" aria-modal="true" aria-labelledby="clip-inspector-title"><div className="player-heading"><div><span className="eyebrow">Selected capture</span><strong id="clip-inspector-title">{clip.title}</strong></div><button className="quiet-button" type="button" onClick={onClose}>Close</button></div>{source ? <video className="clip-video" controls preload="metadata" src={source} aria-label={`Preview ${clip.title}`} /> : <div className="state-panel"><strong>Preview available in the desktop build</strong></div>}<p className="clip-path">{clip.path}</p><div className="inspector-meta"><span>{formatDuration(clip.durationMs)}</span><span>{new Date(clip.createdAt * 1000).toLocaleString()}</span><span>MP4 video</span></div><div className="trim-controls"><label htmlFor="trim-start">Start (seconds)<input id="trim-start" type="number" min="0" max={Math.max(0.1, clip.durationMs / 1000)} step="0.1" value={start} onChange={(event) => setStart(Number(event.target.value))} /></label><label htmlFor="trim-end">End (seconds)<input id="trim-end" type="number" min="0.1" max={Math.max(0.1, clip.durationMs / 1000)} step="0.1" value={end} onChange={(event) => setEnd(Number(event.target.value))} /></label><button className="outline-button" type="button" disabled={trimming} onClick={() => void trimClip()}>{trimming ? 'Trimming…' : 'Save trimmed copy'}</button></div>{trimError && <p className="trim-error" role="alert">{trimError}</p>}<div className="inspector-actions"><button className="primary-button" type="button" onClick={() => onOpen(clip.path)}><Icon name="play" size={14} />Open clip</button><button className="outline-button" type="button" onClick={() => onCopyPath(clip.path)}>Copy path</button><button className="outline-button" type="button" onClick={() => onReveal(clip.path)}><Icon name="folder" size={14} />Reveal folder</button><button className="danger-button" type="button" onClick={() => onDelete(clip)}><Icon name="trash" size={14} />Delete</button></div></div>
}

function InfoBlock({ title, value, icon }: { title: string; value: string; icon: IconName }) { return <div className="info-block"><span><Icon name={icon} size={13} />{title}</span><strong title={value}>{value}</strong></div> }

function formatDuration(durationMs: number) { const seconds = Math.max(0, Math.round(durationMs / 1000)); return `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}` }

function NavButton({ active, onClick, icon, children }: { active: boolean; onClick: () => void; icon: IconName; children: string }) { return <button className={`nav-button ${active ? 'active' : ''}`} type="button" onClick={onClick}><Icon name={icon} size={16} />{children}</button> }

function Icon({ name, size = 16 }: { name: IconName; size?: number }) {
  const common = { width: size, height: size, viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', strokeWidth: 1.8, strokeLinecap: 'round' as const, strokeLinejoin: 'round' as const, 'aria-hidden': true }
  const paths: Record<IconName, ReactNode> = {
    library: <><rect x="3" y="4" width="7" height="7" rx="1" /><rect x="14" y="4" width="7" height="7" rx="1" /><rect x="3" y="14" width="7" height="7" rx="1" /><rect x="14" y="14" width="7" height="7" rx="1" /></>,
    capture: <><circle cx="12" cy="12" r="7" /><circle cx="12" cy="12" r="2.5" /></>,
    settings: <><path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M18.4 5.6l-2.1 2.1M7.7 16.3l-2.1 2.1" /><circle cx="12" cy="12" r="3.5" /></>,
    save: <><path d="M5 3h11l3 3v15H5z" /><path d="M8 3v6h7V3M8 21v-7h8v7" /></>,
    play: <path d="m9 6 9 6-9 6z" fill="currentColor" stroke="none" />,
    folder: <><path d="M3 7.5A1.5 1.5 0 0 1 4.5 6H10l2 2h7.5A1.5 1.5 0 0 1 21 9.5v8A1.5 1.5 0 0 1 19.5 19h-15A1.5 1.5 0 0 1 3 17.5z" /><path d="M3 10h18" /></>,
    trash: <><path d="M4 7h16M10 11v5M14 11v5M9 7V4h6v3M6 7l1 14h10l1-14" /></>,
    refresh: <><path d="M20 11a8 8 0 0 0-14-4L4 9" /><path d="M4 4v5h5M4 13a8 8 0 0 0 14 4l2-2" /><path d="M20 20v-5h-5" /></>,
    microphone: <><rect x="8" y="3" width="8" height="12" rx="4" /><path d="M5 11a7 7 0 0 0 14 0M12 18v3M8 21h8" /></>,
    keyboard: <><rect x="3" y="6" width="18" height="12" rx="2" /><path d="M6 10h.01M9 10h.01M12 10h.01M15 10h.01M18 10h.01M7 14h10" /></>,
    voice: <><path d="M12 4a3 3 0 0 0-3 3v5a3 3 0 0 0 6 0V7a3 3 0 0 0-3-3Z" /><path d="M5 11a7 7 0 0 0 14 0M12 18v3M9 21h6" /></>,
    chevron: <path d="m9 6 6 6-6 6" />,
    check: <path d="m5 12 4 4L19 6" />,
    monitor: <><rect x="3" y="4" width="18" height="13" rx="2" /><path d="M8 21h8M12 17v4" /></>,
    audio: <><path d="M4 10v4M8 7v10M12 4v16M16 7v10M20 10v4" /></>,
    cursor: <path d="m5 3 13 9-6 1 3 6-2.5 1-3-6-4 4z" />,
    info: <><circle cx="12" cy="12" r="9" /><path d="M12 11v5M12 8h.01" /></>,
  }
  return <svg {...common}>{paths[name]}</svg>
}

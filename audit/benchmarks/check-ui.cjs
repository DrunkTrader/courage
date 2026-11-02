// Optional browser smoke. Install Playwright outside the app and set
// COURAGE_PLAYWRIGHT_MODULE to its module path. Run from the repository root.
// IPC is mocked: this does NOT validate native speech, WebView2 or capture.
const { chromium } = require(process.env.COURAGE_PLAYWRIGHT_MODULE || 'playwright')
const { spawn } = require('node:child_process')
const assert = require('node:assert/strict')

const server = spawn('python3', ['-m', 'http.server', '1421', '--bind', '127.0.0.1', '--directory', 'apps/desktop/dist'], { stdio: 'ignore' })
;(async () => {
  let browser
  try {
    browser = await chromium.launch({ executablePath: process.env.COURAGE_CHROMIUM || undefined, headless: true, args: process.env.COURAGE_CHROMIUM ? ['--no-sandbox'] : [] })
    const context = await browser.newContext({ permissions: ['clipboard-read', 'clipboard-write'] })
    const page = await context.newPage()
    const errors = []
    page.on('pageerror', error => errors.push(error.message))
    await page.addInitScript(() => {
      window.calls = []
      let id = 0
      const voice = { generation: null, engine: 'System.Speech · constrained grammar', locale: 'en-US', microphone: 'Windows default recording device', audioFormat: null, availability: 'unchecked', listening: false, testMode: false, error: null }
      const settings = { outputDir: 'C:\\Clips', defaultOutputDir: 'C:\\Clips', audioDevices: [], selectedAudioDevice: null, hotkey: 'Control+Shift+1', replaySeconds: 30, voiceEnabled: true, nativeVoiceAvailable: true, fps: 30, resolutionPreset: 'native', width: null, height: null, videoQuality: 'balanced', videoEncoder: 'auto', hardwareAcceleration: 'auto', cursorCapture: true, audioEnabled: false, audioSampleRate: 48000, audioChannels: 2, captureSource: 'desktop', voiceWakePhrase: 'Courage', voiceCommandPhrase: 'have it', voiceWakeOnly: false, voiceLocale: 'en-US', theme: 'dark' }
      const status = { revision: 1, state: 'recording', sessionId: 'fixture-session', pid: 123, error: null, voice, platform: 'UI fixture', backend: 'fixture only', audioDevice: null, voiceTrigger: 'waiting', outputDir: settings.outputDir, replaySeconds: 30, fps: 30, resolution: 'Native display', videoEncoder: 'libx264', cursorCapture: true }
      window.__TAURI_INTERNALS__ = {
        transformCallback: () => ++id,
        unregisterCallback: () => {},
        invoke: async (cmd, args) => {
          window.calls.push({ cmd, args })
          if (cmd === 'get_settings' || cmd === 'save_settings') return settings
          if (cmd === 'list_clips') return []
          if (cmd === 'engine_status' || cmd === 'start_capture') return status
          if (cmd === 'test_voice_phrase') return { normalized: '', wakeMatched: false, commandMatched: false, wouldTrigger: false, reason: 'fixture response' }
          if (cmd === 'start_voice_test' || cmd === 'stop_voice_test') return voice
          return ++id
        },
      }
    })
    await page.goto('http://127.0.0.1:1421')
    await page.getByRole('button', { name: 'Settings', exact: true }).click()
    const wake = page.locator('#voice-wake'), command = page.locator('#voice-command')
    await wake.fill('Echo')
    await command.fill('')
    await page.getByRole('button', { name: 'Test text', exact: true }).click()
    await page.getByText(/Text simulation only/).waitFor()
    let call = await page.evaluate(() => window.calls.filter(c => c.cmd === 'test_voice_phrase').at(-1))
    assert.equal(call.args.config.wakePhrase, 'Echo')
    assert.equal(call.args.config.commandPhrase, '')
    assert.equal(await wake.getAttribute('autocomplete'), 'off')
    await command.fill('save now')
    await page.getByRole('button', { name: 'Test microphone (no saves)', exact: true }).click()
    await page.waitForFunction(() => window.calls.some(c => c.cmd === 'start_voice_test'))
    call = await page.evaluate(() => window.calls.filter(c => c.cmd === 'start_voice_test').at(-1))
    assert.equal(call.args.config.wakePhrase, 'Echo')
    assert.equal(call.args.config.commandPhrase, 'save now')
    assert.equal(await page.evaluate(() => window.calls.filter(c => c.cmd === 'save_replay').length), 0)
    await wake.focus(); await wake.press('Control+a'); await wake.press('Control+c')
    await command.focus(); await command.press('Control+a'); await command.press('Control+v')
    assert.equal(await command.inputValue(), 'Echo')
    await wake.focus(); await wake.press('Tab')
    assert.equal(await command.evaluate(el => document.activeElement === el), true)
    for (const theme of ['light', 'rusty', 'dark']) {
      await page.getByRole('button', { name: new RegExp('^' + theme, 'i') }).click()
      assert.equal(await page.locator('html').getAttribute('data-theme'), theme)
    }
    assert.deepEqual(errors, [])
    console.log('PASS: browser draft-IPC/no-save action, autocomplete hints, copy/paste, Tab, themes. Native Windows behavior NOT tested.')
  } finally {
    if (browser) await browser.close()
    server.kill()
  }
})().catch(error => { console.error(error); process.exitCode = 1 })

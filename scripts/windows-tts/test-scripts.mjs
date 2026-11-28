#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..', '..');
const windowsTtsDir = path.join(repoRoot, 'scripts', 'windows-tts');
const deploymentDocPath = path.join(repoRoot, 'docs', 'windows-tts-deployment.md');

const EXPECTED_PS_SCRIPTS = [
  'deploy-tts-server.ps1',
  'start-tts-server.ps1',
  'stop-tts-server.ps1',
  'setup-firewall.ps1',
  'install-autostart.ps1',
  'smoke-test.ps1',
];

const EXPECTED_PERSONAS = [
  { id: 'QuickHothead', name: '大川', legacyName: '急眼快攻哥', speaker: 'Eric' },
  { id: 'TeaSarcasm', name: '小晚', legacyName: '阴阳茶艺师', speaker: 'Serena' },
  { id: 'StubbornBlamer', name: '大鹏', legacyName: '嘴硬死磕王', speaker: 'Dylan' },
  { id: 'ChaosInstigator', name: '小桃', legacyName: '拱火乐子人', speaker: 'Vivian' },
  { id: 'TiltedMaster', name: '老陈', legacyName: '红温老雀神', speaker: 'Uncle_Fu' },
];

const EXPECTED_MOODS = ['Smug', 'Mocking', 'Tilted', 'Nervous', 'Plotting', 'Deadpan'];

function readScript(filename) {
  const fullPath = path.join(windowsTtsDir, filename);
  assert.ok(fs.existsSync(fullPath), `Missing script: ${fullPath}`);
  return fs.readFileSync(fullPath, 'utf8').replace(/^\uFEFF/, '');
}

/**
 * Lightweight PowerShell 7 lexical balance validator.
 * Handles # comments, <# ... #> block comments, single/double quotes,
 * here-strings (@'...'@ / @"..."@), backtick escapes, and $(...) inside double quotes.
 */
function validatePowerShellBalance(source, filename) {
  const text = source.replace(/^\uFEFF/, '');
  const len = text.length;
  const stack = [];

  const openToClose = { '{': '}', '(': ')', '[': ']' };
  const closeToOpen = { '}': '{', ')': '(', ']': '[' };

  function lineOf(idx) {
    let line = 1;
    for (let k = 0; k < idx; k++) {
      if (text[k] === '\n') line++;
    }
    return line;
  }

  function scanDoubleQuoted(startIdx) {
    let i = startIdx + 1;
    while (i < len) {
      const ch = text[i];
      if (ch === '`') {
        i += 2;
        continue;
      }
      if (ch === '"' && text[i + 1] === '"') {
        i += 2;
        continue;
      }
      if (ch === '"') {
        return i + 1;
      }
      if (ch === '$' && text[i + 1] === '(') {
        i = scanCode(i + 2, ')');
        continue;
      }
      i++;
    }
    throw new Error(`${filename}:${lineOf(startIdx)}: unclosed double-quoted string`);
  }

  function scanCode(startIdx, stopChar = null) {
    const baseDepth = stack.length;
    let i = startIdx;
    while (i < len) {
      const ch = text[i];
      const next = text[i + 1];

      if (stopChar !== null && ch === stopChar && stack.length === baseDepth) {
        return i + 1;
      }

      if (ch === '<' && next === '#') {
        const end = text.indexOf('#>', i + 2);
        if (end === -1) {
          throw new Error(`${filename}:${lineOf(i)}: unclosed block comment <#`);
        }
        i = end + 2;
        continue;
      }

      if (ch === '#') {
        const nl = text.indexOf('\n', i + 1);
        i = nl === -1 ? len : nl + 1;
        continue;
      }

      if (
        ch === '@' &&
        (next === "'" || next === '"') &&
        (text[i + 2] === '\n' || (text[i + 2] === '\r' && text[i + 3] === '\n'))
      ) {
        const quote = next;
        const bodyStart = text[i + 2] === '\r' ? i + 4 : i + 3;
        const terminator = `\n${quote}@`;
        let pos = bodyStart;
        let found = -1;
        while (pos < len) {
          if (pos === bodyStart && text[pos] === quote && text[pos + 1] === '@') {
            found = pos + 2;
            break;
          }
          const idx = text.indexOf(terminator, pos);
          if (idx === -1) break;
          found = idx + terminator.length;
          break;
        }
        if (found === -1) {
          throw new Error(`${filename}:${lineOf(i)}: unclosed here-string @${quote}`);
        }
        i = found;
        continue;
      }

      if (ch === "'") {
        const start = i;
        i++;
        let closed = false;
        while (i < len) {
          if (text[i] === "'" && text[i + 1] === "'") {
            i += 2;
            continue;
          }
          if (text[i] === "'") {
            i++;
            closed = true;
            break;
          }
          i++;
        }
        if (!closed) {
          throw new Error(`${filename}:${lineOf(start)}: unclosed single-quoted string`);
        }
        continue;
      }

      if (ch === '"') {
        i = scanDoubleQuoted(i);
        continue;
      }

      if (ch === '`') {
        i += 2;
        continue;
      }

      if (openToClose[ch]) {
        stack.push({ ch, line: lineOf(i) });
        i++;
        continue;
      }

      if (closeToOpen[ch]) {
        if (stack.length === 0) {
          throw new Error(`${filename}:${lineOf(i)}: unexpected closing '${ch}'`);
        }
        const top = stack.pop();
        if (openToClose[top.ch] !== ch) {
          throw new Error(
            `${filename}:${lineOf(i)}: mismatched '${ch}' for '${top.ch}' opened at line ${top.line}`,
          );
        }
        i++;
        continue;
      }

      i++;
    }

    if (stopChar !== null) {
      throw new Error(`${filename}:${lineOf(startIdx)}: unclosed subexpression $(...)`);
    }
    return i;
  }

  scanCode(0, null);
  if (stack.length > 0) {
    const unclosed = stack[stack.length - 1];
    throw new Error(`${filename}:${unclosed.line}: unclosed '${unclosed.ch}'`);
  }
}

test('all expected Windows TTS PowerShell scripts exist and pass lexical balance & strict-mode checks', () => {
  for (const filename of EXPECTED_PS_SCRIPTS) {
    const content = readScript(filename);
    assert.doesNotThrow(
      () => validatePowerShellBalance(content, filename),
      `${filename} must have balanced PowerShell delimiters and strings`,
    );
    assert.match(content, /^#Requires -Version 7\.0/m, `${filename} must require PowerShell 7.0+`);
    assert.match(
      content,
      /\$ErrorActionPreference\s*=\s*'Stop'/,
      `${filename} must set $ErrorActionPreference = 'Stop'`,
    );
    assert.match(
      content,
      /Set-StrictMode\s+-Version\s+Latest/,
      `${filename} must enable Set-StrictMode -Version Latest`,
    );
    assert.doesNotMatch(
      content,
      /\bdocker\s+run\b|\bwsl\.exe\b/i,
      `${filename} must stay native green Windows (no WSL2 or Docker Desktop)`,
    );
  }
});

test('deploy-tts-server.ps1 sets up isolated uv green directory, pluggable backends, SDPA (no flash-attn), and VRAM guardrail #1 notice', () => {
  const content = readScript('deploy-tts-server.ps1');

  for (const param of [
    '$RootDir',
    '$Backend',
    '$ModelRepo',
    '$PythonVersion',
    '$SkipVenv',
    '$SkipModel',
    '$ForceRedownload',
  ]) {
    assert.ok(content.includes(param), `deploy-tts-server.ps1 missing parameter ${param}`);
  }

  assert.ok(content.includes('E:\\majjj-tts'), 'must default to E:\\majjj-tts');
  assert.ok(content.includes('qwen3-custom-voice'), 'must support primary qwen3-custom-voice backend');
  assert.ok(content.includes('cosyvoice3'), 'must support runner-up cosyvoice3 backend');
  assert.ok(content.includes('qwen3-base-clone'), 'must support fallback qwen3-base-clone backend');
  assert.ok(content.includes('Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice'), 'must default to Qwen3-TTS-12Hz-0.6B-CustomVoice');
  assert.ok(content.includes('.venv'), 'must create isolated .venv inside RootDir');
  assert.match(content, /\buv\b/, 'must manage isolated Python venv with uv');
  assert.ok(content.includes('Prefer No Sysmem Fallback'), 'must remind operator of Guardrail #1 NVIDIA Control Panel setting');
  assert.doesNotMatch(content, /pip\s+install[^\n]*flash-attn/i, 'must not install flash-attn on Windows');
});

test('start-tts-server.ps1 enforces detached Win32_Process launch, port 8081, backend swap, and VRAM guardrails #2-#4', () => {
  const content = readScript('start-tts-server.ps1');

  for (const param of [
    '$RootDir',
    '$Backend',
    '$ModelPath',
    '$BindHost',
    '$Port',
    '$VramFraction',
    '$MaxNewTokens',
    '$Restart',
    '$Foreground',
  ]) {
    assert.ok(content.includes(param), `start-tts-server.ps1 missing parameter ${param}`);
  }

  assert.match(content, /\$Port\s*=\s*8081\b/, 'default Port must be 8081');
  assert.match(content, /\$VramFraction\s*=\s*0\.30\b/, 'default VramFraction must be 0.30');
  assert.match(content, /\$MaxNewTokens\s*=\s*120\b/, 'default MaxNewTokens must be 120');
  assert.ok(
    content.includes('max_split_size_mb:128,garbage_collection_threshold:0.8'),
    'must configure Windows-safe PYTORCH_CUDA_ALLOC_CONF',
  );
  assert.doesNotMatch(
    content,
    /expandable_segments\s*:\s*true/i,
    'must never enable expandable_segments on Windows WDDM',
  );
  assert.match(
    content,
    /\$PSNativeCommandArgumentPassing\s*=\s*'Standard'/,
    'foreground mode must set $PSNativeCommandArgumentPassing = Standard',
  );
  assert.ok(content.includes('Win32_Process'), 'background mode must detach via Win32_Process.Create');
  assert.ok(content.includes('/health'), 'must poll /health readiness endpoint');
});

test('stop-tts-server.ps1 provides clean idempotent stop for port 8081 TTS process', () => {
  const content = readScript('stop-tts-server.ps1');
  assert.ok(content.includes('$RootDir'), 'stop-tts-server.ps1 must accept -RootDir');
  assert.ok(content.includes('$Port'), 'stop-tts-server.ps1 must accept -Port');
  assert.match(content, /\$Port\s*=\s*8081\b/, 'default Port must be 8081');
  assert.ok(content.includes('tts-server.pid'), 'must read tts-server.pid');
  assert.ok(content.includes('Stop-Process'), 'must terminate TTS process via Stop-Process');
});

test('setup-firewall.ps1 enforces idempotent LAN-only inbound rule on TCP port 8081 and rejects wildcards', () => {
  const content = readScript('setup-firewall.ps1');

  for (const param of ['$RuleName', '$Port', '$LanSubnet', '$Remove']) {
    assert.ok(content.includes(param), `setup-firewall.ps1 missing parameter ${param}`);
  }

  assert.ok(
    content.includes('majjj-tts-lan-8081'),
    'default firewall rule name must be majjj-tts-lan-8081',
  );
  assert.match(content, /\$Port\s*=\s*8081\b/, 'default Port must be 8081');
  assert.ok(
    content.includes('192.168.5.0/255.255.255.0'),
    'default LanSubnet must be 192.168.5.0/255.255.255.0',
  );
  assert.ok(content.includes('New-NetFirewallRule'), 'must create rule with New-NetFirewallRule');
  assert.ok(content.includes('Remove-NetFirewallRule'), 'must support idempotent cleanup/removal');
  assert.match(content, /-Direction\s+Inbound/, 'firewall rule must be Inbound');
  assert.match(content, /-Protocol\s+TCP/, 'firewall rule must restrict Protocol to TCP');
  assert.match(content, /-RemoteAddress\s+\$LanSubnet/, 'firewall rule must restrict RemoteAddress to $LanSubnet');
  assert.ok(content.includes('0.0.0.0/0'), 'must guard against wildcard 0.0.0.0/0 subnet');
  assert.match(content, /\bAny\b/, 'must guard against wildcard Any remote address');
});

test('install-autostart.ps1 provides idempotent SYSTEM AtStartup scheduled task install and uninstall', () => {
  const content = readScript('install-autostart.ps1');

  for (const param of ['$TaskName', '$RootDir', '$Backend', '$ModelPath', '$Port', '$Uninstall']) {
    assert.ok(content.includes(param), `install-autostart.ps1 missing parameter ${param}`);
  }

  assert.ok(content.includes('MajjjTtsServer'), 'default TaskName must be MajjjTtsServer');
  assert.ok(content.includes('Register-ScheduledTask'), 'must register scheduled task');
  assert.ok(content.includes('Unregister-ScheduledTask'), 'must support uninstalling scheduled task');
  assert.ok(content.includes('-AtStartup'), 'must trigger at system startup');
  assert.ok(content.includes('SYSTEM'), 'must run under SYSTEM account');
  assert.ok(content.includes('start-tts-server.ps1'), 'scheduled task must invoke start-tts-server.ps1');
});

test('smoke-test.ps1 exercises /health, 5 personas x 6 moods WAV synthesis, deadline drop, and 30-line stress + VRAM check', () => {
  const content = readScript('smoke-test.ps1');

  for (const param of ['$BaseUrl', '$OutputDir', '$SkipStress', '$StressCount']) {
    assert.ok(content.includes(param), `smoke-test.ps1 missing parameter ${param}`);
  }

  assert.ok(content.includes('/health'), 'must check /health');
  assert.ok(content.includes('/v1/tts/synthesize'), 'must test /v1/tts/synthesize');
  assert.ok(content.includes('/v1/audio/speech'), 'must support probing /v1/audio/speech');
  assert.ok(content.includes('RIFF'), 'must validate WAV RIFF header');
  assert.ok(content.includes('X-TTS-Duration-Ms'), 'must inspect diagnostic headers');
  assert.ok(content.includes('deadline_ms'), 'must exercise deadline_ms expiry guardrail');
  assert.ok(content.includes('nvidia-smi'), 'must check GPU VRAM budget via nvidia-smi when available');
  assert.ok(content.includes('10800'), 'must check 10,800 MiB total VRAM co-residency threshold');

  for (const mood of EXPECTED_MOODS) {
    assert.ok(content.includes(`'${mood}'`) || content.includes(`"${mood}"`), `smoke-test.ps1 missing mood ${mood}`);
  }
  for (const persona of EXPECTED_PERSONAS) {
    assert.ok(content.includes(persona.id), `smoke-test.ps1 missing persona id ${persona.id}`);
    assert.ok(content.includes(persona.name), `smoke-test.ps1 missing persona name ${persona.name}`);
  }
});

test('personas.json defines all 5 personas x 6 moods with exact speaker mapping', () => {
  const personasPath = path.join(windowsTtsDir, 'server', 'personas.json');
  assert.ok(fs.existsSync(personasPath), `Missing personas.json: ${personasPath}`);
  const data = JSON.parse(fs.readFileSync(personasPath, 'utf8'));

  assert.equal(data.sample_rate, 24000, 'personas.json sample_rate must be 24000');
  for (const persona of EXPECTED_PERSONAS) {
    const entry = data.personas[persona.id];
    assert.ok(entry, `personas.json missing persona ${persona.id}`);
    assert.equal(entry.display_name, persona.name);
    assert.equal(entry.speaker, persona.speaker);
    assert.ok(
      Array.isArray(entry.aliases) && entry.aliases.includes(persona.legacyName),
      `personas.json ${persona.id} must retain legacy alias ${persona.legacyName}`,
    );
  }
  for (const mood of EXPECTED_MOODS) {
    const entry = data.moods[mood];
    assert.ok(entry, `personas.json missing mood ${mood}`);
    assert.ok(entry.instruct && entry.instruct.trim().length > 0, `mood ${mood} must define instruct`);
  }
});

test('docs/windows-tts-deployment.md documents all scripts, 5 VRAM guardrails, pluggable backends, and BENDER benchmark template', () => {
  assert.ok(fs.existsSync(deploymentDocPath), `Missing documentation: ${deploymentDocPath}`);
  const doc = fs.readFileSync(deploymentDocPath, 'utf8');

  for (const filename of EXPECTED_PS_SCRIPTS) {
    assert.ok(doc.includes(filename), `docs/windows-tts-deployment.md must mention ${filename}`);
  }

  for (const marker of [
    'E:\\majjj-tts',
    'majjj-tts-lan-8081',
    '8081',
    'Prefer No Sysmem Fallback',
    'set_per_process_memory_fraction(0.30',
    'max_split_size_mb:128,garbage_collection_threshold:0.8',
    'max_new_tokens',
    'deadline_ms',
    'Qwen3-TTS-12Hz-0.6B-CustomVoice',
    'Fun-CosyVoice3-0.5B-2512',
    'Qwen3-TTS-12Hz-0.6B-Base',
    '10,800 MiB',
    '3,500 MiB',
    'BENDER',
  ]) {
    assert.ok(doc.includes(marker), `docs/windows-tts-deployment.md missing expected detail: ${marker}`);
  }
});

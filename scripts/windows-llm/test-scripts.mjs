#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const repoRoot = path.resolve(__dirname, '..', '..');
const windowsLlmDir = path.join(repoRoot, 'scripts', 'windows-llm');
const deploymentDocPath = path.join(repoRoot, 'docs', 'windows-llm-deployment.md');

const EXPECTED_PS_SCRIPTS = [
  'deploy-llama-server.ps1',
  'start-llama-server.ps1',
  'stop-llama-server.ps1',
  'setup-firewall.ps1',
  'install-autostart.ps1',
  'smoke-test.ps1',
];

const EXPECTED_MOODS = ['Smug', 'Mocking', 'Tilted', 'Nervous', 'Plotting', 'Deadpan'];
const EXPECTED_PERSONAS = [
  '急眼快攻哥',
  '阴阳茶艺师',
  '嘴硬死磕王',
  '拱火乐子人',
  '红温老雀神',
];

function readScript(filename) {
  const fullPath = path.join(windowsLlmDir, filename);
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

      // Block comment <# ... #>
      if (ch === '<' && next === '#') {
        const end = text.indexOf('#>', i + 2);
        if (end === -1) {
          throw new Error(`${filename}:${lineOf(i)}: unclosed block comment <#`);
        }
        i = end + 2;
        continue;
      }

      // Single-line comment # ...
      if (ch === '#') {
        const nl = text.indexOf('\n', i + 1);
        i = nl === -1 ? len : nl + 1;
        continue;
      }

      // Here-string @' ... '@ or @" ... "@
      if (ch === '@' && (next === "'" || next === '"') && (text[i + 2] === '\n' || (text[i + 2] === '\r' && text[i + 3] === '\n'))) {
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

      // Single-quoted string '...'
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

      // Double-quoted string "..."
      if (ch === '"') {
        i = scanDoubleQuoted(i);
        continue;
      }

      // Backtick escape in code
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

test('PowerShell lexical balance validator accepts nested subexpressions and rejects malformed syntax', () => {
  assert.doesNotThrow(() =>
    validatePowerShellBalance('Write-Host "val=$([Math]::Round(1.234, 2))"\n', 'valid.ps1'),
  );
  assert.throws(
    () => validatePowerShellBalance('if ($true) { Write-Host "oops"\n', 'unclosed-brace.ps1'),
    /unclosed '\{'/,
  );
  assert.throws(
    () => validatePowerShellBalance('Write-Host "unterminated\n', 'unclosed-str.ps1'),
    /unclosed double-quoted string/,
  );
});

test('all expected PowerShell scripts exist and pass lexical balance & strict-mode checks', () => {
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
  }
});

test('deploy-llama-server.ps1 supports idempotent green CUDA & Qwen3.5-9B-Q5_K_M deployment with SHA-256 & chunked download', () => {
  const content = readScript('deploy-llama-server.ps1');

  // Configurable parameters
  for (const param of ['$RootDir', '$LlamaReleaseTag', '$ModelFileName', '$ExpectedModelSha256', '$ForceRedownload']) {
    assert.ok(content.includes(param), `deploy-llama-server.ps1 missing parameter ${param}`);
  }

  // Verified official artifacts and hashes from Ticket #65 BENDER deployment
  assert.ok(content.includes('ggml-org/llama.cpp'), 'must reference official ggml-org/llama.cpp releases');
  assert.ok(content.includes('b12760'), 'must default to verified release b12760');
  assert.ok(content.includes('cudart-llama-bin-win-cuda'), 'must fetch matching CUDA runtime zip');
  assert.ok(content.includes('Qwen3.5-9B-Q5_K_M.gguf'), 'must default to Qwen3.5-9B-Q5_K_M.gguf');
  assert.ok(
    content.includes('dc2a39aef291f91a9116ad214058da0d86eb648743a124bd8c333787c4b9c91c'),
    'must include verified Qwen3.5-9B-Q5_K_M.gguf SHA-256 checksum',
  );
  assert.ok(content.includes('6577841376'), 'must include verified Qwen3.5-9B-Q5_K_M.gguf byte size');

  // SHA-256 verification and resumable Range chunked download
  assert.match(content, /Get-FileHash\b.*SHA256/s, 'must verify SHA-256 with Get-FileHash');
  assert.match(content, /Range|RangeHeaderValue/i, 'must support resumable HTTP Range chunk downloads');
});

test('start-llama-server.ps1 uses verified RTX 3060 flags, model swap parameter, detached Win32_Process launch, and /health + /props check', () => {
  const content = readScript('start-llama-server.ps1');

  for (const param of [
    '$RootDir',
    '$ModelPath',
    '$BindHost',
    '$Port',
    '$ParallelSlots',
    '$ContextSize',
    '$Restart',
    '$Foreground',
  ]) {
    assert.ok(content.includes(param), `start-llama-server.ps1 missing parameter ${param}`);
  }

  // Verified defaults & critical flags
  assert.match(content, /\$ParallelSlots\s*=\s*3\b/, 'default ParallelSlots must be 3');
  assert.match(content, /\$ContextSize\s*=\s*12288\b/, 'default ContextSize must be 12288');
  assert.ok(content.includes('-ngl'), 'must pass -ngl GPU offload flag');
  assert.ok(content.includes('999'), 'must default GPU offload layers to 999');
  assert.ok(content.includes('-fa'), 'must pass -fa Flash Attention flag');
  assert.ok(content.includes('--parallel'), 'must pass --parallel flag');
  assert.ok(content.includes('-c'), 'must pass explicit -c context size flag');
  assert.ok(content.includes('--jinja'), 'must enable --jinja template engine');
  assert.ok(content.includes('--chat-template-kwargs'), 'must pass --chat-template-kwargs');
  assert.ok(content.includes('enable_thinking'), 'must explicitly disable enable_thinking');
  assert.ok(content.includes('--reasoning-budget'), 'must pass --reasoning-budget');
  assert.ok(content.includes('--log-file'), 'must configure --log-file');

  // PowerShell 7 quote passing & SSH-safe detached process launch
  assert.match(
    content,
    /\$PSNativeCommandArgumentPassing\s*=\s*'Standard'/,
    'foreground mode must set $PSNativeCommandArgumentPassing = Standard',
  );
  assert.ok(content.includes('Win32_Process'), 'background mode must detach via Win32_Process.Create');
  assert.ok(content.includes('/health'), 'must poll /health readiness endpoint');
  assert.ok(content.includes('/props'), 'must inspect /props after startup');
});

test('stop-llama-server.ps1 provides clean idempotent stop', () => {
  const content = readScript('stop-llama-server.ps1');
  assert.ok(content.includes('$RootDir'), 'stop-llama-server.ps1 must accept -RootDir');
  assert.ok(content.includes('$Port'), 'stop-llama-server.ps1 must accept -Port');
  assert.ok(content.includes('llama-server'), 'must target llama-server process');
  assert.ok(content.includes('Stop-Process'), 'must terminate llama-server via Stop-Process');
});

test('setup-firewall.ps1 enforces idempotent LAN-only inbound rule on TCP port 8080', () => {
  const content = readScript('setup-firewall.ps1');

  for (const param of ['$RuleName', '$Port', '$LanSubnet', '$Remove']) {
    assert.ok(content.includes(param), `setup-firewall.ps1 missing parameter ${param}`);
  }

  assert.ok(
    content.includes('majjj-llama-server-lan-8080'),
    'default firewall rule name must be majjj-llama-server-lan-8080',
  );
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

test('install-autostart.ps1 provides idempotent scheduled task install and uninstall', () => {
  const content = readScript('install-autostart.ps1');

  for (const param of ['$TaskName', '$RootDir', '$ModelPath', '$Uninstall']) {
    assert.ok(content.includes(param), `install-autostart.ps1 missing parameter ${param}`);
  }

  assert.ok(content.includes('Register-ScheduledTask'), 'must register scheduled task');
  assert.ok(content.includes('Unregister-ScheduledTask'), 'must support uninstalling scheduled task');
  assert.ok(content.includes('-AtStartup'), 'must trigger at system startup');
  assert.ok(content.includes('SYSTEM'), 'must run under SYSTEM account');
  assert.ok(content.includes('start-llama-server.ps1'), 'scheduled task must invoke start-llama-server.ps1');
});

test('smoke-test.ps1 verifies /health, /props or /v1/models, strict 6-mood json_schema, zero think leakage, 5 personas, latency & concurrency', () => {
  const content = readScript('smoke-test.ps1');

  for (const param of ['$BaseUrl', '$SkipConcurrency', '$Concurrency']) {
    assert.ok(content.includes(param), `smoke-test.ps1 missing parameter ${param}`);
  }

  assert.ok(content.includes('/health'), 'must check /health');
  assert.ok(content.includes('/props') || content.includes('/v1/models'), 'must check /props or /v1/models');
  assert.ok(content.includes('/v1/chat/completions'), 'must test /v1/chat/completions');
  assert.ok(content.includes('json_schema'), 'must enforce OpenAI-compatible json_schema');
  assert.ok(content.includes('additionalProperties'), 'schema must set additionalProperties = false');

  for (const mood of EXPECTED_MOODS) {
    assert.ok(content.includes(`'${mood}'`) || content.includes(`"${mood}"`), `smoke-test.ps1 missing mood ${mood}`);
  }

  for (const persona of EXPECTED_PERSONAS) {
    assert.ok(content.includes(persona), `smoke-test.ps1 missing persona ${persona}`);
  }

  assert.ok(content.includes('<think>'), 'must check for <think> leakage');
  assert.ok(content.includes('reasoning_content'), 'must check for reasoning_content leakage');
  assert.ok(content.includes('TTFT'), 'must report TTFT latency');
  assert.ok(content.includes('-Parallel'), 'must support concurrent smoke requests');
});

test('docs/windows-llm-deployment.md documents all scripts, model swap, troubleshooting, and BENDER benchmark summary', () => {
  assert.ok(fs.existsSync(deploymentDocPath), `Missing documentation: ${deploymentDocPath}`);
  const doc = fs.readFileSync(deploymentDocPath, 'utf8');

  for (const filename of EXPECTED_PS_SCRIPTS) {
    assert.ok(doc.includes(filename), `docs/windows-llm-deployment.md must mention ${filename}`);
  }

  for (const marker of [
    '-ModelPath',
    'BENDER',
    'RTX 3060',
    'b12760',
    'Qwen3.5-9B-Q5_K_M.gguf',
    '12288',
    '7,229 MiB',
    '7,237 MiB',
    '155.7 ms',
    '860.3 ms',
    'majjj-llama-server-lan-8080',
  ]) {
    assert.ok(doc.includes(marker), `docs/windows-llm-deployment.md missing expected benchmark/ops detail: ${marker}`);
  }
});

test('all scripts/windows-llm/ files and docs/windows-llm-deployment.md are free of GLOSSARY.md forbidden synonyms', () => {
  const glossaryCheckScript = fs.readFileSync(path.join(repoRoot, 'scripts', 'check-glossary.sh'), 'utf8');
  const zhMatch = glossaryCheckScript.match(/^forbidden_zh='([^']+)'/m);
  const robMatch = glossaryCheckScript.match(/^forbidden_rob='([^']+)'/m);
  assert.ok(zhMatch, 'failed to extract forbidden_zh from scripts/check-glossary.sh');
  assert.ok(robMatch, 'failed to extract forbidden_rob from scripts/check-glossary.sh');

  const combinedRe = new RegExp(`${zhMatch[1]}|${robMatch[1]}`, 'g');
  const filesToCheck = [
    ...EXPECTED_PS_SCRIPTS.map((name) => path.join(windowsLlmDir, name)),
    deploymentDocPath,
  ];

  for (const filePath of filesToCheck) {
    const content = fs.readFileSync(filePath, 'utf8');
    const matches = content.match(combinedRe);
    assert.equal(
      matches,
      null,
      `Forbidden GLOSSARY.md synonym(s) found in ${path.relative(repoRoot, filePath)}: ${JSON.stringify(matches)}`,
    );
  }
});

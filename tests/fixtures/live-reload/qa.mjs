#!/usr/bin/env node
// Writer client QA over the lifted presenter DevTools transport, no packages.
// node tests/fixtures/live-reload/qa.mjs [--only ABCDEFGHIJK] [--chrome PATH]
//   [--out DIR] [--assets PATH] [--client PATH] [--pages-client PATH]
// Node >=22 (fetch/WebSocket), Python3, and Chrome/Chromium are required.

import { spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const argv = process.argv.slice(2);
const option = (name, fallback) => {
    const index = argv.indexOf(name);
    return index < 0 ? fallback : argv[index + 1];
};
const ONLY = option('--only', 'ABCDEFGHIJK');
const OUT = option('--out', null);
const CHROME = option('--chrome', process.env.CHROME_PATH || (
    process.platform === 'darwin'
        ? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
        : 'google-chrome'
));
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const results = [];

function installHelpers() {
    window.qa = {
        wait: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
        state: () => window.__writerLiveReload.state,
        async until(predicate, ms = 8000) {
            const end = Date.now() + ms;
            while (Date.now() < end) {
                try {
                    if (predicate()) return true;
                } catch { /* page or build not ready yet */ }
                await qa.wait(25);
            }
            return false;
        },
        async get(path) {
            return (await fetch(path)).text();
        },
        async post(path, body) {
            return (await fetch(path, { method: 'POST', body })).text();
        },
        async document() {
            return qa.get('/__mock/document');
        },
        async build(query = 'kind=content') {
            const event = JSON.parse(await qa.get('/__mock/emit?' + query));
            if (!await qa.until(() => qa.state().buildId === event.build_id)) {
                throw new Error('build not applied: ' + JSON.stringify({
                    event, state: qa.state(),
                }));
            }
            await qa.wait(100);
            return event;
        },
        async edit(change, query = 'kind=content') {
            await qa.post('/__mock/document', change(await qa.document()));
            return qa.build(query);
        },
        async log() {
            return JSON.parse(await qa.get('/__mock/log'));
        },
        overlay() {
            const element = document.getElementById('writer-overlay');
            return {
                text: element?.textContent || '',
                shown: !!element && getComputedStyle(element).display !== 'none'
                    && !element.hidden,
                count: document.querySelectorAll('#writer-overlay').length,
            };
        },
        firstVisible() {
            return [...document.querySelectorAll('[data-sourcepos]')].find(
                (element) => {
                    const top = element.getBoundingClientRect().top;
                    return top >= 0 && top < innerHeight;
                }
            );
        },
        centered(id) {
            const rect = document.getElementById(id).getBoundingClientRect();
            return Math.abs((rect.top + rect.bottom - innerHeight) / 2) <= 1;
        },
        nav: () => performance.getEntriesByType('navigation')[0].type,
        key: () => 'writer:build_id:' + location.pathname + location.search,
    };
    return true;
}

async function cssSwap() {
    window.__qaMarker = 'css';
    let flash = false;
    const observer = new MutationObserver(() => {
        const links = [...document.querySelectorAll('link#writer-theme')];
        if (!links.some((link) => link.sheet)) flash = true;
    });
    observer.observe(document.head, { childList: true });
    await qa.log();
    await qa.post('/__mock/css', (
        await fetch('/__writer/theme.css').then((response) => response.text())
    ) + '\nbody { color: rgb(1, 2, 3); }');
    await qa.post('/__mock/asset?path=extra.css', 'body { --qa-extra: 9; }');
    const event = await qa.build('kind=css');
    observer.disconnect();
    const links = [...document.querySelectorAll('link#writer-theme')];
    const extra = document.querySelector('link[href*="extra.css"]');
    const log = await qa.log();
    return {
        item: '1 CSS and theme-chain links swap after loading, without a flash',
        pass: window.__qaMarker === 'css' && !flash && links.length === 1
            && links[0].getAttribute('href').endsWith('?v=' + event.build_id)
            && extra.getAttribute('href').endsWith('?v=' + event.build_id)
            && getComputedStyle(document.body).color === 'rgb(1, 2, 3)'
            && getComputedStyle(document.body).getPropertyValue('--qa-extra')
                .trim() === '9'
            && !log.some((entry) => entry.startsWith('GET /?view=web ')),
        flash, log,
    };
}

async function anchorEdits() {
    window.__qaMarker = 'anchor';
    window.scrollTo(0, document.getElementById('p30').offsetTop - 120);
    const first = qa.firstVisible();
    const before = first.getBoundingClientRect().top;
    await qa.edit((html) => html.replace('Paragraph 31:', 'Edited paragraph 31:'));
    const visibleDelta = first.getBoundingClientRect().top - before;
    await qa.edit((html) => '<nav style="height:300px">New TOC</nav>' + html);
    const tocDelta = first.getBoundingClientRect().top - before;
    const removed = qa.firstVisible();
    const replacement = removed.nextElementSibling;
    const removedTop = removed.getBoundingClientRect().top;
    await qa.edit((html) => {
        const template = document.createElement('template');
        template.innerHTML = html;
        template.content.querySelector('#' + removed.id).remove();
        return template.innerHTML;
    });
    const deleteDelta = replacement.getBoundingClientRect().top - removedTop;
    return {
        item: '2,4 visible edit, TOC insertion, and deleted anchor retain ±1px',
        pass: window.__qaMarker === 'anchor'
            && [visibleDelta, tocDelta, deleteDelta].every((d) => Math.abs(d) <= 1),
        visibleDelta, tocDelta, deleteDelta,
    };
}

async function scrollEdits() {
    window.scrollTo(0, document.getElementById('p40').offsetTop);
    await qa.edit((html) => html.replace('Paragraph 20:', 'Changed above:'),
        'kind=content&source=0&line=200');
    const above = qa.centered('p20');
    await qa.edit((html) => html.replace('Paragraph 50:', 'Changed below:'),
        'kind=content&source=0&line=500');
    const below = qa.centered('p50');
    window.scrollTo(0, 0);
    await qa.build('kind=content&source=1&line=41&file=chapter.md');
    const include = qa.centered('included-narrow');
    window.scrollTo(0, 0);
    await qa.build('kind=content&source=2&cell=3&file=book.ipynb');
    const cell = qa.centered('cell3');
    return {
        item: '3,5 offscreen edits use source, smallest line range, and cell',
        pass: above && below && include && cell, above, below, include, cell,
    };
}

async function mathAndDetails() {
    const math = document.getElementById('math');
    await qa.until(() => math.dataset.rendered === '1');
    const original = math.innerHTML;
    math.__qa = 1;
    document.getElementById('details').open = true;
    await qa.edit((html) => html.replace('id="math"',
        'id="math" data-source="0" data-sourcepos="120-120"'));
    const kept = document.getElementById('math') === math && math.__qa === 1
        && math.innerHTML === original && math.dataset.sourcepos === '120-120'
        && math.dataset.rendered === '1';
    const details = document.getElementById('details').open;
    await qa.edit((html) => html.replace('data-tex="x^2"', 'data-tex="x^3"')
        .replace('>x^2</span>', '>x^3</span>'));
    const updated = document.getElementById('math');
    return {
        item: '6 unchanged KaTeX identity, edited rendering, reader details state',
        pass: kept && details && updated.dataset.rendered === '1'
            && updated.innerHTML !== original && !!updated.querySelector('.katex'),
        kept, details,
    };
}

async function imageEdit() {
    window.__qaMarker = 'images';
    const first = document.getElementById('image1');
    const second = document.getElementById('image2');
    first.__qa = 1;
    second.__qa = 2;
    await qa.log();
    await qa.post('/__mock/image?name=000000000001',
        '<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40">'
        + '<rect width="80" height="40" fill="green"/></svg>');
    await qa.edit((html) => html.replace('000000000001?v=1', '000000000001?v=2')
        .replace('alt="Second"', 'alt="Updated second"'));
    const log = await qa.log();
    await qa.until(() => document.getElementById('image1').complete);
    return {
        item: '7 changed image version loads; unchanged image keeps identity',
        pass: window.__qaMarker === 'images' && second === document.getElementById('image2')
            && second.__qa === 2 && second.alt === 'Updated second'
            && log.some((line) => line.includes('000000000001?v=2'))
            && !log.some((line) => line.includes('000000000002'))
            && document.getElementById('image1').naturalWidth > 0,
        log,
    };
}

async function titleEdit() {
    window.__qaMarker = 'title';
    await qa.get('/__mock/title?t=Changed%20title');
    await qa.build();
    return {
        item: '8 document title updates in place',
        pass: document.title === 'Changed title' && window.__qaMarker === 'title',
    };
}

async function errorsAndWarnings() {
    window.__qaMarker = 'errors';
    await qa.build('kind=content&web=error&pages=error&cell=3&line=7&message='
        + encodeURIComponent('Long error '.repeat(120)));
    const initial = qa.overlay();
    const element = document.getElementById('writer-overlay');
    const rect = element.getBoundingClientRect();
    const error = initial.shown && initial.count === 1
        && initial.text.includes('[web, pages] build error (parse)')
        && initial.text.includes('document.md cell 3:7')
        && initial.text.includes(':::include <missing> & file.md')
        && initial.text.includes('hint: Fix <the> source & save')
        && !element.querySelector('missing')
        && getComputedStyle(element).position === 'fixed'
        && rect.width <= innerWidth / 2 + 1;
    element.click();
    const dismissed = !qa.overlay().shown;
    const errorId = qa.state().errorId;
    await qa.get('/__mock/down?ms=300');
    await qa.until(() => !qa.state().connected);
    await qa.until(() => qa.state().connected);
    await qa.wait(300);
    const kept = !qa.overlay().shown && qa.state().errorId === errorId;
    await qa.build('kind=content&web=ok&pages=error&error_kind=compile');
    const pagesError = qa.overlay().text.includes('[pages] build error (compile)')
        && qa.overlay().shown && qa.state().views.web.status === 'ok'
        && document.getElementById('writer-view-toggle').dataset.status === 'error'
        && document.getElementById('writer-view-toggle').textContent.includes('●');
    const css = await qa.build('kind=css');
    const cssKeepsError = qa.state().views.pages.status === 'error'
        && qa.overlay().text.includes('[pages] build error (compile)')
        && document.getElementById('writer-theme').href.endsWith('?v=' + css.build_id);
    await qa.build('kind=content&warning=1');
    const warnings = document.getElementById('writer-warnings');
    const counts = warnings.textContent.includes('1 warning(s)')
        && warnings.textContent.includes('1 error(s)');
    warnings.click();
    const expanded = await qa.until(() => !!warnings.querySelector('ol'));
    const warningText = warnings.textContent;
    const warningSafe = warningText.includes('unresolved-reference')
        && warningText.includes('label <nope>')
        && warningText.includes('book.ipynb cell 3:2')
        && !warnings.querySelector('nope');
    await qa.build('kind=content&warning=1');
    const persistent = !!document.getElementById('writer-warnings').querySelector('ol')
        && sessionStorage.getItem('writer:warnings-open') !== null;
    await qa.build('kind=content&warning=0');
    const cleared = !qa.overlay().shown
        && (document.getElementById('writer-warnings').hidden
            || getComputedStyle(document.getElementById('writer-warnings')).display === 'none');
    return {
        item: '10–12 errors, dismissal/reconnect, skipped view error, warning details',
        pass: error && dismissed && kept && pagesError && cssKeepsError
            && counts && expanded && warningSafe && persistent && cleared
            && window.__qaMarker === 'errors',
        error, dismissed, kept, pagesError, cssKeepsError,
        counts, expanded, warningSafe, persistent, cleared,
    };
}

async function staleFetch() {
    window.__qaMarker = 'stale';
    const oldId = qa.state().buildId;
    await qa.post('/__mock/document', (await qa.document())
        .replace('Paragraph 2:', 'Recovered stale content:'));
    await qa.get('/__mock/failroot?on=1');
    await qa.get('/__mock/emit?kind=content');
    const stale = await qa.until(() => qa.state().lastStatus === 'stale');
    const held = qa.state().buildId === oldId
        && !document.body.textContent.includes('Recovered stale content:');
    await qa.get('/__mock/failroot?on=0');
    await qa.build('kind=css');
    return {
        item: 'failed content fetch defers storage and retries content on CSS build',
        pass: stale && held && window.__qaMarker === 'stale'
            && document.getElementById('p2').textContent.includes('Recovered stale'),
        stale, held,
    };
}

async function reconnect() {
    window.__qaMarker = 'reconnect';
    await qa.get('/__mock/down?ms=1400');
    const down = await qa.until(() => !qa.state().connected);
    await qa.post('/__mock/document', (await qa.document())
        .replace('Paragraph 1:', 'Missed build:'));
    await qa.get('/__mock/emit?kind=content');
    const back = await qa.until(() => qa.state().connected
        && document.getElementById('p1').textContent.includes('Missed build:'));
    await qa.get('/__mock/down?ms=400');
    await qa.until(() => !qa.state().connected);
    await qa.post('/__mock/document', (await qa.document())
        .replace('Missed build:', 'Restarted at same id:'));
    await qa.get('/__mock/restart');
    const restarted = await qa.until(() => qa.state().connected
        && qa.state().buildId === 1
        && document.getElementById('p1').textContent.includes('Restarted at same id:'));
    return {
        item: '13 reconnect applies missed builds and restart ID in place',
        pass: down && back && restarted && window.__qaMarker === 'reconnect',
        down, back, restarted,
    };
}

async function backoff() {
    window.__qaMarker = 'backoff';
    const delays = [];
    const recorder = setInterval(() => {
        const state = qa.state();
        if (!state.connected && delays.at(-1) !== state.retryMs) {
            delays.push(state.retryMs);
        }
    }, 10);
    const start = performance.now();
    const connectionIndex = window.__qaConnections.length;
    await qa.get('/__mock/down?ms=16500');
    const lost = await qa.until(() => !qa.state().connected, 1000);
    const eventually = await qa.until(() => qa.state().connected, 23000);
    clearInterval(recorder);
    const expected = [1000, 2000, 4000, 5000];
    const attempts = [start, ...window.__qaConnections.slice(connectionIndex)];
    const elapsed = attempts.slice(1).map((time, index) => time - attempts[index]);
    const timing = [500, 1000, 2000, 4000, 5000].every((delay, index) =>
        elapsed[index] >= delay * 0.7
        && elapsed[index] <= delay * 1.4 + 150);
    return {
        item: '13 outage retries at 0.5/1/2/4 seconds then caps at 5 seconds',
        pass: lost && eventually && timing && expected.every((n) => delays.includes(n))
            && window.__qaMarker === 'backoff',
        delays, elapsed,
    };
}

async function pagesDelegation() {
    const before = window.__qaPages.length;
    const first = await qa.build('kind=pages');
    const applied = window.__qaPages.at(-1);
    const count = window.__qaPages.length;
    await qa.build('kind=css');
    const skipped = window.__qaPages.length === count;
    await qa.get('/__mock/down?ms=300');
    await qa.until(() => !qa.state().connected);
    await qa.get('/__mock/restart');
    const restart = await qa.until(() => window.__qaPages.at(-1)?.id === 1);
    return {
        item: '11 pages view delegates pages events, skips CSS, accepts restart ID',
        pass: count === before + 1 && applied.id === first.build_id
            && applied.pages.count === 2 && skipped && restart,
        skipped, restart,
    };
}

async function keyGuards() {
    const before = location.href;
    for (const tag of ['input', 'textarea', 'select', 'div']) {
        const element = document.createElement(tag);
        if (tag === 'div') element.contentEditable = 'true';
        document.body.append(element);
        element.dispatchEvent(new KeyboardEvent('keydown', {
            key: 'p', bubbles: true,
        }));
        element.remove();
    }
    for (const modifier of ['ctrlKey', 'metaKey', 'altKey']) {
        document.body.dispatchEvent(new KeyboardEvent('keydown', {
            key: 'p', bubbles: true, [modifier]: true,
        }));
    }
    const prevented = new KeyboardEvent('keydown', {
        key: 'p', bubbles: true, cancelable: true,
    });
    prevented.preventDefault();
    document.body.dispatchEvent(prevented);
    await qa.wait(150);
    return {
        item: '14 typing, modifiers and prevented p do not switch views',
        pass: location.href === before,
    };
}

// Lifted from presenter_rs's package-free Chrome DevTools transport.
async function startMock() {
    const env = { ...process.env };
    if (option('--assets', null)) env.WRITER_ASSETS = option('--assets');
    if (option('--client', null)) env.WRITER_CLIENT = option('--client');
    if (option('--pages-client', null)) env.WRITER_PAGES_CLIENT = option('--pages-client');
    const processHandle = spawn('python3', [
        '-B', join(HERE, 'mock-server.py'), '--port', '0',
    ], { env, stdio: ['ignore', 'pipe', 'inherit'] });
    const base = await new Promise((resolve, reject) => {
        let output = '';
        const timeout = setTimeout(() => reject(new Error('mock startup timed out')), 10000);
        processHandle.stdout.on('data', (data) => {
            output += data;
            const match = output.match(/http:\/\/127\.0\.0\.1:\d+/);
            if (match) {
                clearTimeout(timeout);
                resolve(match[0]);
            }
        });
        processHandle.once('error', reject);
        processHandle.once('exit', (code) => reject(new Error('mock exited: ' + code)));
    });
    return {
        base,
        async stop() {
            if (processHandle.exitCode !== null) return;
            await new Promise((resolve) => {
                processHandle.once('exit', resolve);
                processHandle.kill();
            });
        },
    };
}

async function launchChrome() {
    const profile = mkdtempSync(join(tmpdir(), 'writer-qa-'));
    const processHandle = spawn(CHROME, [
        '--headless=new', '--disable-gpu', '--no-first-run',
        '--no-default-browser-check', '--disable-extensions',
        '--disable-background-networking', '--disable-sync',
        '--remote-debugging-port=0', `--user-data-dir=${profile}`,
        '--window-size=1280,720', 'about:blank',
    ], { stdio: ['ignore', 'ignore', 'pipe'] });
    const port = await new Promise((resolve, reject) => {
        let output = '';
        const timeout = setTimeout(() => reject(new Error('Chrome startup timed out')), 15000);
        processHandle.stderr.on('data', (data) => {
            output += data;
            const match = output.match(/DevTools listening on ws:\/\/[^:]+:(\d+)\//);
            if (match) {
                clearTimeout(timeout);
                resolve(+match[1]);
            }
        });
        processHandle.once('error', reject);
        processHandle.once('exit', (code) => reject(new Error('Chrome exited: ' + code)));
    });
    const target = await (await fetch(`http://127.0.0.1:${port}/json/new?about:blank`, {
        method: 'PUT',
    })).json();
    const socket = new WebSocket(target.webSocketDebuggerUrl);
    await new Promise((resolve, reject) => {
        socket.onopen = resolve;
        socket.onerror = reject;
    });
    let nextId = 0;
    let loads = 0;
    const pending = new Map();
    const logs = [];
    socket.onmessage = (event) => {
        const message = JSON.parse(event.data);
        if (message.id && pending.has(message.id)) {
            const request = pending.get(message.id);
            pending.delete(message.id);
            clearTimeout(request.timeout);
            if (message.error) request.reject(new Error(JSON.stringify(message.error)));
            else request.resolve(message.result);
        } else if (message.method === 'Page.loadEventFired') {
            loads++;
        } else if (message.method === 'Runtime.consoleAPICalled') {
            logs.push('console.' + message.params.type + ': ' + message.params.args
                .map((arg) => arg.value ?? arg.description ?? arg.type).join(' '));
        } else if (message.method === 'Runtime.exceptionThrown') {
            const detail = message.params.exceptionDetails;
            logs.push('EXCEPTION: ' + (detail.exception?.description || detail.text));
        }
    };
    const send = (method, params = {}) => new Promise((resolve, reject) => {
        const id = ++nextId;
        const timeout = setTimeout(() => {
            pending.delete(id);
            reject(new Error('DevTools timeout: ' + method));
        }, 35000);
        pending.set(id, { resolve, reject, timeout });
        socket.send(JSON.stringify({ id, method, params }));
    });
    await send('Page.enable');
    await send('Runtime.enable');
    await send('Page.addScriptToEvaluateOnNewDocument', {
        source: `window.__qaConnections = [];
            window.EventSource = class extends window.EventSource {
                constructor(url, options) {
                    super(url, options);
                    window.__qaConnections.push(performance.now());
                }
            };`,
    });
    await send('Emulation.setDeviceMetricsOverride', {
        width: 1280, height: 720, deviceScaleFactor: 1, mobile: false,
    });
    const page = {
        logs,
        get loads() { return loads; },
        async evaluate(expression) {
            const result = await send('Runtime.evaluate', {
                expression, awaitPromise: true, returnByValue: true,
            });
            if (result.exceptionDetails) {
                throw new Error(result.exceptionDetails.exception?.description
                    || result.exceptionDetails.text);
            }
            return result.result.value;
        },
        async run(fn) {
            return page.evaluate(`(${fn})()`);
        },
        async loaded(minimum) {
            const end = Date.now() + 10000;
            while (loads < minimum && Date.now() < end) await sleep(25);
            if (loads < minimum) throw new Error('expected page load did not occur');
            await page.ready();
        },
        async ready() {
            const end = Date.now() + 10000;
            while (Date.now() < end) {
                try {
                    const ready = await page.evaluate(
                        '!!window.__writerLiveReload?.state.connected'
                    );
                    if (ready) {
                        await sleep(200);
                        await page.run(installHelpers);
                        return;
                    }
                } catch { /* navigation replaced the context */ }
                await sleep(25);
            }
            throw new Error('live reload did not connect');
        },
        async goto(url) {
            const old = loads;
            await send('Page.navigate', { url });
            await page.loaded(old + 1);
        },
        async navigating(fn) {
            const old = loads;
            try {
                await page.run(fn);
            } catch (error) {
                if (!/context|navigat|Cannot find|Inspected target/i.test(error.message)) {
                    throw error;
                }
            }
            await page.loaded(old + 1);
        },
        async screenshot(path) {
            const output = await send('Page.captureScreenshot', { format: 'png' });
            writeFileSync(path, Buffer.from(output.data, 'base64'));
        },
        async close() {
            socket.close();
            await new Promise((resolve) => {
                processHandle.once('exit', resolve);
                processHandle.kill('SIGKILL');
            });
            rmSync(profile, { recursive: true, force: true });
        },
    };
    return page;
}

async function record(page, fn) {
    const result = await page.run(fn);
    if (!result || typeof result.pass !== 'boolean') {
        throw new Error('scenario did not return a verdict: ' + fn.name);
    }
    results.push(result);
}

async function navigationScenarios(page, mock) {
    const old = page.loads;
    await page.navigating(async () => {
        window.__qaMarker = 'outside';
        await qa.post('/__mock/head', '<style>body { background: #fef; }</style>');
        await qa.get('/__mock/emit?kind=content');
    });
    await sleep(300);
    results.push({
        item: '8 template change outside root reloads exactly once',
        pass: page.loads === old + 1 && await page.run(() => !window.__qaMarker
            && !!document.head.querySelector('style')),
    });
    const scriptLoads = page.loads;
    await page.navigating(async () => {
        await qa.post('/__mock/document', (await qa.document())
            + '<script>sessionStorage.setItem("qa:script", '
            + 'String(+(sessionStorage.getItem("qa:script") || 0) + 1))</script>');
        await qa.get('/__mock/emit?kind=content');
    });
    await page.navigating(async () => {
        await qa.get('/__mock/emit?kind=content');
    });
    results.push({
        item: '9 raw root script reloads on every content build',
        pass: page.loads === scriptLoads + 2 && await page.run(() =>
            sessionStorage.getItem('qa:script') === '2'),
    });
    // A new page load in the same tab has the stale per-URL session key.
    await page.run(() => sessionStorage.setItem(qa.key(), '0'));
    const staleLoads = page.loads;
    await page.goto(mock.base + '/?view=web');
    await sleep(300);
    results.push({
        item: '13 fresh page with stale stored ID reloads once and stores latest',
        pass: page.loads === staleLoads + 2 && await page.run(() =>
            sessionStorage.getItem(qa.key()) === String(qa.state().buildId)),
    });
}

async function fallbackScenarios(page, mock) {
    await fetch(mock.base + '/__mock/emit?kind=content&web=error&pages=error&nodoc=1');
    await page.goto(mock.base + '/?view=web');
    const fallback = await page.run(async () => {
        const initial = qa.overlay();
        document.getElementById('writer-overlay').click();
        await qa.get('/__mock/down?ms=300');
        await qa.until(() => !qa.state().connected);
        await qa.until(() => qa.state().connected);
        await qa.wait(250);
        return initial.shown && initial.count === 1
            && initial.text.includes('[web, pages] build error (parse)')
            && !qa.overlay().shown && !document.querySelector('[data-writer-document]');
    });
    await page.navigating(async () => {
        window.__qaMarker = 'fallback';
        await qa.get('/__mock/emit?kind=content');
    });
    results.push({
        item: '15 fallback dismisses across reconnect; first good build reloads',
        pass: fallback && await page.run(() => !window.__qaMarker
            && !!document.querySelector('[data-writer-document]')),
    });
}

async function viewScenarios(page, mock) {
    await record(page, keyGuards);
    await page.navigating(() => document.body.dispatchEvent(new KeyboardEvent(
        'keydown', { key: 'p', bubbles: true }
    )));
    const pages = await page.run(() => location.search === '?view=pages'
        && qa.state().view === 'pages'
        && sessionStorage.getItem('writer:view') === 'pages');
    await record(page, pagesDelegation);
    await page.navigating(() => document.getElementById('writer-view-toggle').click());
    const web = await page.run(() => location.search === '?view=web'
        && document.getElementById('writer-view-toggle').textContent === 'Pages');
    await page.goto(mock.base + '/?view=pages');
    await page.goto(mock.base + '/');
    const remembered = await page.run(() => location.search === '?view=pages');
    await page.goto(mock.base + '/?view=web');
    const explicit = await page.run(() => qa.state().view === 'web'
        && sessionStorage.getItem('writer:view') === 'web');
    results.push({
        item: '14 p/button toggle views; root remembers last view; query wins',
        pass: pages && web && remembered && explicit,
        pages, web, remembered, explicit,
    });
}

async function headLoadedClient(page, mock) {
    const web = await page.run(async () => {
        const head = !!document.head.querySelector('script[src="/__writer/live-reload.js"]');
        window.__qaMarker = 'head-loaded';
        await qa.edit((html) => html.replace('Paragraph 1:', 'Head-loaded edit:'));
        return head && qa.state().view === 'web'
            && document.getElementById('writer-view-toggle').textContent === 'Pages'
            && window.__qaMarker === 'head-loaded'
            && document.getElementById('p1').textContent.includes('Head-loaded edit:');
    });
    await page.goto(mock.base + '/?view=pages');
    const pages = await page.run(async () => {
        const before = window.__qaPages.length;
        const event = await qa.build('kind=pages');
        return qa.state().view === 'pages'
            && document.getElementById('writer-view-toggle').textContent === 'Web'
            && window.__qaPages.length === before + 1
            && window.__qaPages.at(-1).id === event.build_id;
    });
    results.push({
        item: 'head-loaded client waits for body and detects late web/pages markers',
        pass: web && pages, web, pages,
    });
}

async function installPagesHelpers() {
    const data = await fetch('/__writer/state', { cache: 'no-store' })
        .then((response) => response.json());
    window.qp = {
        current: JSON.parse(JSON.stringify(data.pages)),
        nodes: () => [...document.querySelectorAll('#writer-pages > .writer-page')],
        hash: (number) => number.toString(16).padStart(16, '0'),
        payload(count, seed = 0x1000) {
            return {
                status: 'ok', count,
                hashes: Array.from({ length: count }, (_, i) => qp.hash(seed + i)),
                size_pt: Array.from({ length: count }, () => [450, 600]),
            };
        },
        async frame() {
            await new Promise((resolve) => requestAnimationFrame(resolve));
            await new Promise((resolve) => requestAnimationFrame(resolve));
        },
        async stage(spec, query = 'kind=pages') {
            qp.current = JSON.parse(JSON.stringify(spec));
            await qa.post('/__mock/pages', JSON.stringify(spec));
            return qa.build(query);
        },
        anchor() {
            const index = qp.nodes().findIndex((node) =>
                node.getBoundingClientRect().bottom > 0);
            const node = qp.nodes()[index];
            if (!node) return null;
            const rect = node.getBoundingClientRect();
            return { index, fraction: -rect.top / rect.height, top: rect.top };
        },
        async scroll(index, fraction = .3) {
            const node = qp.nodes()[index];
            const rect = node.getBoundingClientRect();
            scrollTo(0, scrollY + rect.top + rect.height * fraction);
            await qp.frame();
        },
        loaded(index, hash = null) {
            const images = qp.nodes()[index]?.querySelectorAll('img');
            if (!images || images.length !== 1) return false;
            const image = images[0];
            return image.complete && image.naturalWidth > 0 && (!hash
                || new URL(image.src).searchParams.get('v') === hash);
        },
        requested(entry, hash) {
            return entry.startsWith('GET /__writer/pages/')
                && entry.includes('?v=' + hash + ' ');
        },
        async requestsUntil(hash) {
            const deadline = Date.now() + 8000;
            const entries = [];
            do {
                entries.push(...await qa.log());
                if (entries.some((entry) => qp.requested(entry, hash))) {
                    return { found: true, entries };
                }
                await qa.wait(25);
            } while (Date.now() < deadline);
            return { found: false, entries };
        },
        isolated(source, fetcher, blockedStorage = false) {
            const frame = document.createElement('iframe');
            frame.style.cssText = 'position:fixed;right:0;top:0;width:400px;height:240px';
            frame.title = 'Isolated pages client contract';
            document.body.append(frame);
            const win = frame.contentWindow;
            win.document.open();
            win.document.write('<!DOCTYPE html><html><body>'
                + '<header id="writer-pages-toolbar">'
                + '<button data-zoom="out">-</button>'
                + '<span id="writer-pages-zoom"></span>'
                + '<button data-zoom="in">+</button>'
                + '<button data-zoom="fit">Fit</button>'
                + '<span id="writer-pages-counter"></span>'
                + '<span id="writer-pages-status" hidden></span></header>'
                + '<main id="writer-pages" data-writer-pages></main>'
                + '</body></html>');
            win.document.close();
            win.fetch = fetcher;
            if (blockedStorage) {
                Object.defineProperty(win, 'sessionStorage', { get() {
                    throw new DOMException('deliberately blocked', 'SecurityError');
                }});
            }
            win.eval(source);
            return frame;
        },
    };
    return qa.until(() => qp.nodes().length === data.pages.count
        && qp.loaded(0));
}

async function pagesLayoutAndAnchor() {
    const nodes = qp.nodes();
    const first = nodes[0].querySelector('img');
    const geometry = nodes.every((node, index) => {
        const image = node.querySelector('img');
        const rect = node.getBoundingClientRect();
        return node.dataset.page === String(index + 1)
            && node.dataset.hash === qp.current.hashes[index]
            && image.alt === `Page ${index + 1}`
            && image.loading === 'lazy' && image.decoding === 'async'
            && !image.draggable && image.getAttribute('width') === '600'
            && image.getAttribute('height') === '800'
            && rect.width > 0 && rect.height > 0
            && Math.abs(rect.width - parseFloat(image.style.width)) <= 1
            && Math.abs(rect.height - parseFloat(image.style.height)) <= 1;
    });
    const initialLog = await qa.log();
    const farUnrequested = !initialLog.some((entry) =>
        qp.requested(entry, qp.current.hashes.at(-1)));
    await qp.scroll(6);
    const ready = await qa.until(() => qp.loaded(6));
    const page = qp.nodes()[6];
    const image = page.querySelector('img');
    const anchor = qp.anchor();
    await qa.log();
    const changed = JSON.parse(JSON.stringify(qp.current));
    changed.hashes[4] = qp.hash(0x2005);
    await qp.stage(changed);
    await qp.frame();
    const after = qp.anchor();
    const log = await qa.log();
    const pageRequests = log.filter((entry) =>
        entry.startsWith('GET /__writer/pages/'));
    const preserved = after?.index === anchor?.index
        && Math.abs(after.top - anchor.top) <= 1
        && qp.nodes()[6] === page && page.querySelector('img') === image
        && document.getElementById('writer-pages-counter').textContent === 'Page 7 / 24';
    // Page 5 can be outside native lazy lookahead while reading page 7. Keep
    // the revision-only request check above, then reveal it deliberately.
    // That navigation may also request previously unseen, unchanged pages.
    await qp.scroll(4);
    const fetchLog = [...log];
    if (!fetchLog.some((entry) => qp.requested(entry, changed.hashes[4]))) {
        const request = await qp.requestsUntil(changed.hashes[4]);
        fetchLog.push(...request.entries);
    }
    const changedLoaded = await qa.until(() => qp.loaded(4, changed.hashes[4]));
    fetchLog.push(...await qa.log());
    const changedRequests = fetchLog.filter((entry) =>
        qp.requested(entry, changed.hashes[4]));
    return {
        item: '17 pages are pre-sized/lazy; page 7 survives page 5 revision ±1px',
        pass: geometry && farUnrequested && ready && nodes.length === 24
            && first.complete && window.__qaConnections.length === 1
            && preserved && changedLoaded && changedRequests.length === 1
            && pageRequests.every((entry) => qp.requested(entry, changed.hashes[4])),
        geometry, farUnrequested, anchor, after, preserved,
        pageRequests, changedLoaded, changedRequests,
    };
}

async function pagesStatusAndEmpty() {
    const good = qp.nodes();
    const hashes = good.map((node) => node.dataset.hash);
    await qa.build('kind=pages&pages=error&error_kind=compile');
    const status = document.getElementById('writer-pages-status');
    const retained = !status.hidden
        && status.textContent === 'pages: build error, showing the last good pages'
        && qp.nodes().every((node, index) => node === good[index]
            && node.dataset.hash === hashes[index]);
    await qa.build('kind=pages');
    const cleared = status.hidden;
    await qp.stage(qp.payload(0));
    document.querySelector('[data-zoom="fit"]').click();
    dispatchEvent(new Event('resize'));
    await qa.build('kind=pages&pages=error&error_kind=compile');
    const empty = document.querySelector('.writer-pages-empty');
    return {
        item: '18 pages errors retain last good; empty state and fit stay usable',
        pass: retained && cleared && qp.nodes().length === 0
            && empty?.textContent === 'No pages yet.'
            && document.querySelectorAll('.writer-pages-empty').length === 1
            && document.getElementById('writer-pages-counter').textContent === 'Page 0 / 0'
            && document.getElementById('writer-pages-zoom').textContent === 'Fit'
            && !status.hidden && status.textContent === 'pages: build error, no pages yet',
        retained, cleared,
    };
}

async function pagesZoomAndKeys() {
    const label = () => document.getElementById('writer-pages-zoom').textContent;
    const button = (action) => document.querySelector(`[data-zoom="${action}"]`).click();
    const key = (value, target = document.body, options = {}) => {
        target.dispatchEvent(new KeyboardEvent('keydown', {
            key: value, bubbles: true, cancelable: true, ...options,
        }));
    };
    const grid = document.getElementById('writer-pages');
    await qp.scroll(6);
    const anchor = qp.anchor();
    grid.style.width = '620px';
    dispatchEvent(new Event('resize'));
    const fit = (grid.clientWidth - 32) / 600;
    const actual = parseFloat(qp.nodes()[0].querySelector('img').style.width) / 600;
    const fitCorrect = label() === 'Fit' && Math.abs(fit - actual) < 1e-6;
    const delta = qp.anchor().top - anchor.top * (actual / 2);
    key('+');
    const levels = [.5, .67, .75, .9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3];
    const next = levels.find((level) => level > actual + 1e-9);
    const plus = label() === Math.round(next * 100) + '%';
    key('0');
    const zero = label() === 'Fit';
    key('=');
    const equals = label() === Math.round(next * 100) + '%';
    key('-');
    const minus = label() !== 'Fit' && label() !== Math.round(next * 100) + '%';
    button('out');
    const down = label() !== 'Fit';
    for (let i = 0; i < 20; i += 1) button('in');
    key('+');
    const upper = label() === '300%';
    for (let i = 0; i < 20; i += 1) button('out');
    key('-');
    const lower = label() === '50%';
    button('fit');
    button('in');
    const before = label();
    const guards = [];
    for (const tag of ['input', 'textarea', 'select', 'div']) {
        const target = document.createElement(tag);
        if (tag === 'div') {
            target.contentEditable = 'true';
            target.innerHTML = '<span>editable child</span>';
        }
        document.body.append(target);
        key('+', target.firstElementChild || target);
        guards.push(label() === before);
        target.remove();
    }
    for (const modifier of ['ctrlKey', 'metaKey', 'altKey']) {
        key('+', document.body, { [modifier]: true });
        guards.push(label() === before);
    }
    const prevented = document.createElement('div');
    prevented.addEventListener('keydown', (event) => event.preventDefault());
    document.body.append(prevented);
    key('+', prevented);
    guards.push(label() === before);
    prevented.remove();
    const stored = sessionStorage.getItem('writer:pages-zoom');
    window.__qaStoredZoom = before;
    return {
        item: '19 pages fit/buttons/+ = - 0 preserve anchor and exclude typing/modifiers',
        pass: fitCorrect && Math.abs(delta) <= 1 && plus && zero && equals
            && minus && down && upper && lower && guards.every(Boolean)
            && stored === String(next),
        fit, actual, delta, plus, zero, equals, minus, down, upper, lower,
        guards, stored,
    };
}

async function pagesStartupAndStorage() {
    const source = await qa.get('/__writer/pages.js');
    let release;
    const frame = qp.isolated(source, () => new Promise((resolve) => { release = resolve; }));
    let restart = false;
    let startup = false;
    try {
        const win = frame.contentWindow;
        const older = qp.payload(1, 0x7000);
        const first = qp.payload(4, 0x7100);
        const restarted = qp.payload(2, 0x7200);
        win.__writerPages.apply(first, 18);
        win.__writerPages.apply(restarted, 1);
        restart = win.document.querySelectorAll('.writer-page').length === 2
            && win.document.querySelector('.writer-page').dataset.hash === restarted.hashes[0];
        release({ ok: true, json: () => Promise.resolve({ pages: older, build_id: 99 }) });
        await qa.wait(0);
        startup = win.document.querySelectorAll('.writer-page').length === 2
            && win.document.querySelector('.writer-page').dataset.hash === restarted.hashes[0];
    } finally {
        frame.remove();
    }
    const payload = qp.payload(1, 0x7300);
    const blocked = qp.isolated(source, () => Promise.resolve({
        ok: true, json: () => Promise.resolve({ pages: payload, build_id: 1 }),
    }), true);
    let storage = false;
    try {
        const win = blocked.contentWindow;
        win.__writerPages.apply(payload, 2);
        win.document.querySelector('[data-zoom="in"]').click();
        storage = win.document.querySelectorAll('.writer-page').length === 1
            && win.document.getElementById('writer-pages-zoom').textContent !== 'Fit';
    } finally {
        blocked.remove();
    }
    return {
        item: '20 pages public apply accepts 18→1; startup fetch cannot overwrite it; storage may fail',
        pass: restart && startup && storage, restart, startup, storage,
    };
}

async function pagesOldImageAndLazyRevision() {
    const index = 23;
    await qp.scroll(index, 0);
    const decoded = await qa.until(() => qp.loaded(index));
    const node = qp.nodes()[index];
    const old = node.querySelector('img');
    scrollTo(0, 0);
    await qp.frame();
    const changed = JSON.parse(JSON.stringify(qp.current));
    const hash = qp.hash(0x3024);
    changed.hashes[index] = hash;
    await qa.get('/__mock/pages/hold?hash=' + hash);
    await qa.log();
    try {
        await qp.stage(changed);
        await qp.frame();
        const log = await qa.log();
        const lazy = !log.some((entry) => qp.requested(entry, hash));
        const kept = node.contains(old) && old.complete && old.naturalWidth > 0
            && node.querySelectorAll('img').length === 2;
        await qp.scroll(index, 0);
        const request = await qp.requestsUntil(hash);
        const pending = node.contains(old) && node.querySelectorAll('img').length === 2
            && getComputedStyle(old).opacity === '1';
        await qa.get('/__mock/pages/release?hash=' + hash);
        const replaced = await qa.until(() => qp.loaded(index, hash));
        return {
            item: '21 changed far decoded page stays lazy and keeps old pixels until load/decode',
            pass: decoded && lazy && kept && request.found && pending && replaced
                && !node.contains(old),
            decoded, lazy, kept, pending, replaced, requests: request.entries,
        };
    } finally {
        await qa.get('/__mock/pages/release?hash=' + hash);
    }
}

async function pagesStaleDecodeAndRevert() {
    const index = 23;
    await qp.scroll(index, 0);
    const node = qp.nodes()[index];
    const originalDecode = HTMLImageElement.prototype.decode;
    const a = qp.hash(0x4024);
    const b = qp.hash(0x5024);
    let releaseDecode = null;
    HTMLImageElement.prototype.decode = function () {
        const decoded = originalDecode.call(this);
        return this.src.endsWith('?v=' + a) ? decoded.then(() => new Promise((resolve) => {
            releaseDecode = resolve;
        })) : decoded;
    };
    let stale = false;
    let reverted = false;
    try {
        let spec = JSON.parse(JSON.stringify(qp.current));
        spec.hashes[index] = a;
        await qp.stage(spec);
        const waiting = await qa.until(() => typeof releaseDecode === 'function');
        spec = JSON.parse(JSON.stringify(qp.current));
        spec.hashes[index] = b;
        await qp.stage(spec);
        const latest = await qa.until(() => qp.loaded(index, b));
        if (releaseDecode) releaseDecode();
        await qp.frame();
        stale = waiting && latest && qp.loaded(index, b);
        const visible = node.querySelector('img');
        const pendingHash = qp.hash(0x6024);
        await qa.get('/__mock/pages/hold?hash=' + pendingHash);
        try {
            spec = JSON.parse(JSON.stringify(qp.current));
            spec.hashes[index] = pendingHash;
            await qp.stage(spec);
            const requested = await qp.requestsUntil(pendingHash);
            await qa.log();
            spec = JSON.parse(JSON.stringify(qp.current));
            spec.hashes[index] = b;
            await qp.stage(spec);
            const log = await qa.log();
            reverted = requested.found && node.querySelectorAll('img').length === 1
                && node.querySelector('img') === visible && qp.loaded(index, b)
                && !log.some((entry) => qp.requested(entry, b));
        } finally {
            await qa.get('/__mock/pages/release?hash=' + pendingHash);
        }
    } finally {
        if (releaseDecode) releaseDecode();
        HTMLImageElement.prototype.decode = originalDecode;
    }
    return {
        item: '22 stale decode cannot replace a newer revision; reverting reuses displayed pixels',
        pass: stale && reverted, stale, reverted,
    };
}

async function openCompilerPages(page, mock) {
    const count = 24;
    await fetch(mock.base + '/__mock/pages', {
        method: 'POST', body: JSON.stringify({
            hashes: Array.from({ length: count }, (_, i) => (0x1000 + i).toString(16).padStart(16, '0')),
            size_pt: Array.from({ length: count }, () => [450, 600]),
        }),
    });
    await fetch(mock.base + '/__mock/emit?kind=pages');
    await page.goto(mock.base + '/?view=pages&client=1');
    if (!await page.run(installPagesHelpers)) throw new Error('pages layout did not become ready');
}

async function pagesGeometryScenarios(page, mock) {
    await openCompilerPages(page, mock);
    await record(page, pagesLayoutAndAnchor);
    await record(page, pagesStatusAndEmpty);
}

async function pagesControlsScenarios(page, mock) {
    await openCompilerPages(page, mock);
    await record(page, pagesZoomAndKeys);
    const expected = await page.run(() => window.__qaStoredZoom);
    await page.goto(mock.base + '/?view=pages&client=1');
    if (!await page.run(installPagesHelpers)) throw new Error('stored zoom layout did not become ready');
    const stored = await page.run(() => document.getElementById('writer-pages-zoom').textContent);
    results.push({
        item: '19 pages zoom choice survives navigation in the same tab',
        pass: stored === expected, expected, stored,
    });
    await record(page, pagesStartupAndStorage);
}

async function pagesRevisionScenarios(page, mock) {
    await openCompilerPages(page, mock);
    await record(page, pagesOldImageAndLazyRevision);
    await record(page, pagesStaleDecodeAndRevert);
}

const SESSIONS = [
    { id: 'A', steps: [cssSwap, anchorEdits] },
    { id: 'B', steps: [scrollEdits, mathAndDetails, imageEdit, titleEdit] },
    { id: 'C', steps: [errorsAndWarnings] },
    { id: 'D', node: navigationScenarios },
    { id: 'E', steps: [reconnect, backoff] },
    { id: 'F', node: viewScenarios },
    { id: 'G', steps: [staleFetch], node: fallbackScenarios },
    { id: 'H', async setup(mock) {
        await fetch(mock.base + '/__mock/early?on=1');
        await fetch(mock.base + '/__mock/emit?kind=content');
    }, node: headLoadedClient },
    { id: 'I', node: pagesGeometryScenarios },
    { id: 'J', node: pagesControlsScenarios },
    { id: 'K', node: pagesRevisionScenarios },
];

async function main() {
    if (OUT) mkdirSync(OUT, { recursive: true });
    const mock = await startMock();
    const logs = [];
    try {
        for (const session of SESSIONS.filter((s) => ONLY.includes(s.id))) {
            process.stderr.write('session ' + session.id + '...\n');
            await fetch(mock.base + '/__mock/reset');
            if (session.setup) await session.setup(mock);
            const page = await launchChrome();
            const firstResult = results.length;
            try {
                await page.goto(mock.base + '/?view=web');
                for (const step of session.steps || []) await record(page, step);
                if (session.node) await session.node(page, mock);
                if (OUT) await page.screenshot(join(OUT, session.id + '.png'));
            } catch (error) {
                results.push({
                    item: session.id + ' scenario exception', pass: false,
                    error: error.stack || String(error),
                });
            } finally {
                logs.push(...page.logs);
                if (OUT) writeFileSync(join(OUT, session.id + '.log'),
                    results.slice(firstResult).map((r) => JSON.stringify(r)).join('\n')
                    + '\n' + page.logs.join('\n') + '\n');
                await page.close();
            }
        }
    } finally {
        await mock.stop();
    }
    const bad = logs.filter((line) => line.startsWith('EXCEPTION')
        || (line.startsWith('console.')
            && !line.startsWith('console.debug: [writer]')));
    results.push({
        item: '16 no uncaught exceptions; client console uses [writer] prefix',
        pass: bad.length === 0, bad,
    });
    for (const result of results) {
        console.log(`${result.pass ? 'PASS' : 'FAIL'}  ${result.item}`);
        if (!result.pass) console.log(JSON.stringify(result));
    }
    const failed = results.filter((result) => !result.pass).length;
    console.log(`${results.length - failed} passed, ${failed} failed`);
    process.exitCode = failed ? 1 : 0;
}

main().catch((error) => {
    console.error('qa.mjs: ' + (error.stack || error));
    process.exitCode = 2;
});

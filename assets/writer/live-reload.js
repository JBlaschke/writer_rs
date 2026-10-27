// Client of `writer serve` (docs/spec/live-reload.md §3); ES2020, no deps.
// Lifted from presenter_rs; debug: window.__writerLiveReload.
(() => {
    'use strict';

    const KEY = 'writer:build_id:' + location.pathname + location.search;
    const state = {
        buildId: null, connected: false, retryMs: 500,
        lastStatus: 'ok', errorId: null,
        view: document.querySelector('[data-writer-pages]') ? 'pages' : 'web',
        views: {
            web: { status: 'ok', error: null },
            pages: { status: 'ok', error: null }
        },
        warnings: []
    };
    let source = null;
    let timer = null;
    const log = (...args) => console.debug('[writer]', ...args);

    // GET / parsed; null if it fails.
    const getDoc = () => fetch('/?view=web', { cache: 'no-store' })
        .then((response) => response.ok ? response.text() : null)
        .then(
            (text) => text && new DOMParser().parseFromString(text, 'text/html'),
            () => null
        );

    // What a morph cannot update: everything outside the document, except
    // title, metadata and content versions. Root attributes are copied later.
    const signature = (doc) => {
        const page = doc.documentElement.cloneNode(true);
        page.querySelectorAll('title,meta').forEach((el) => el.remove());
        const root = page.querySelector('[data-writer-document]');
        if (root) {
            root.replaceChildren();
            Array.from(root.attributes).forEach(({ name }) => {
                if (name !== 'data-writer-document') root.removeAttribute(name);
            });
        }
        return page.outerHTML.replace(/\?v=[0-9A-Za-z]+/g, '');
    };

    // Baseline: the served page, before scripts have changed its DOM.
    let baseline = '';
    const ver = (doc) => doc.querySelector('#writer-theme')?.getAttribute('href');
    let queue = Promise.resolve();

    function store(id) {
        state.buildId = id;
        try {
            sessionStorage.setItem(KEY, id);
        } catch { /* storage disabled */ }
    }

    // Stored first: the reloaded page must not reload again.
    function fullReload(why, id) {
        store(id);
        disconnect();
        log('full reload:', why);
        location.reload();
        return false;
    }

    const overlay = () => document.getElementById('writer-overlay');
    const otherView = () => state.view === 'web' ? 'pages' : 'web';
    let warningsOpen = false;
    try {
        warningsOpen = sessionStorage.getItem('writer:warnings-open') === '1';
    } catch { /* storage disabled */ }

    function locationText(item) {
        if (!item.file) return '';
        return item.file + (item.cell != null ? ' cell ' + item.cell : '') +
            (item.line != null ? ':' + item.line : '');
    }

    function errorGroups() {
        const groups = [];
        ['web', 'pages'].forEach((view) => {
            if (state.views[view].status !== 'error') return;
            const error = state.views[view].error || {};
            const key = JSON.stringify(error);
            const group = groups.find((item) => item.key === key);
            if (group) group.views.push(view);
            else groups.push({ key, error, views: [view] });
        });
        groups.sort((a, b) =>
            Number(b.views.includes(state.view)) -
            Number(a.views.includes(state.view))
        );
        return groups;
    }

    function showOverlay(groups, visible) {
        const old = overlay();
        const el = old || document.body.appendChild(document.createElement('div'));
        const display = old ? old.style.display : 'none';
        el.id = 'writer-overlay';
        el.style.cssText =
            'position:fixed;top:1em;right:1em;box-sizing:border-box;' +
            'max-width:50%;max-height:80vh;' +
            'overflow:auto;z-index:10000;background:#222;color:#eee;' +
            'border-left:4px solid #e55;padding:1em;font:13px monospace;' +
            'white-space:pre-wrap;cursor:pointer';
        el.style.display = visible ? 'block' : display;
        el.onclick = () => { el.style.display = 'none'; };
        el.replaceChildren();
        groups.forEach(({ views, error }, index) => {
            if (index) el.append('\n\n');
            const heading = '[' + views.join(', ') + '] build error' +
                (error.kind ? ' (' + error.kind + ')' : '');
            const lines = [heading, error.message || 'unknown error'];
            const where = locationText(error);
            if (where) lines.push(where);
            el.append(lines.join('\n'));
            if (error.excerpt) {
                const pre = el.appendChild(document.createElement('pre'));
                pre.textContent = error.excerpt;
                pre.style.cssText = 'margin:.5em 0;color:#fc6';
            }
            (error.hints || []).forEach((hint) => el.append('\nhint: ' + hint));
        });
        el.append('\n(click to dismiss)');
    }

    function hideOverlay() {
        const el = overlay();
        if (el) el.style.display = 'none';
    }

    function renderWarnings() {
        let panel = document.getElementById('writer-warnings');
        if (!panel) {
            panel = document.body.appendChild(document.createElement('div'));
            panel.id = 'writer-warnings';
            panel.style.cssText =
                'position:fixed;bottom:1em;left:1em;max-width:70%;' +
                'z-index:9999;background:#222;color:#eee;padding:.6em;' +
                'font:13px monospace';
            panel.onclick = () => {
                warningsOpen = !warningsOpen;
                try {
                    sessionStorage.setItem(
                        'writer:warnings-open', warningsOpen ? '1' : '0'
                    );
                } catch { /* storage disabled */ }
                renderWarnings();
            };
        }
        panel.replaceChildren();
        panel.style.display = state.warnings.length ? 'block' : 'none';
        const warnings = state.warnings.filter(
            (item) => item.level === 'warning'
        ).length;
        const errors = state.warnings.filter(
            (item) => item.level === 'error'
        ).length;
        const counts = [];
        if (warnings) counts.push(warnings + ' warning(s)');
        if (errors) counts.push(errors + ' error(s)');
        const button = panel.appendChild(document.createElement('button'));
        button.type = 'button';
        button.textContent = counts.join(', ');
        button.setAttribute('aria-expanded', String(warningsOpen));
        button.style.cssText =
            'background:none;border:0;color:inherit;font:inherit;cursor:pointer';
        if (warningsOpen) {
            const list = panel.appendChild(document.createElement('ol'));
            list.style.cssText = 'max-height:40vh;overflow:auto;margin:.6em 0 0';
            state.warnings.forEach((warning) => {
                const item = list.appendChild(document.createElement('li'));
                const where = locationText(warning);
                item.textContent = warning.code + ': ' + warning.message +
                    (where ? ' — ' + where : '');
            });
        }
    }

    function renderToggle() {
        let button = document.getElementById('writer-view-toggle');
        if (!button) {
            button = document.body.appendChild(document.createElement('button'));
            button.id = 'writer-view-toggle';
            button.type = 'button';
            button.title = 'Switch view (p)';
            button.style.cssText =
                'position:fixed;bottom:1em;right:1em;z-index:9999;' +
                'background:#222;color:#eee;border:1px solid #888;' +
                'padding:.6em;font:13px monospace;cursor:pointer';
            button.onclick = () => setView(otherView());
        }
        const error = state.views[otherView()].status === 'error';
        button.textContent = (state.view === 'web' ? 'Pages' : 'Web') +
            (error ? ' ●' : '');
        if (error) button.setAttribute('data-status', 'error');
        else button.removeAttribute('data-status');
        button.style.borderColor = error ? '#e55' : '#888';
    }

    function renderStatus(data) {
        ['web', 'pages'].forEach((view) => {
            const next = data[view];
            if (next && next.status !== 'skipped') {
                state.views[view] = {
                    status: next.status, error: next.error || null
                };
            }
        });
        state.warnings = data.warnings || [];
        const groups = errorGroups();
        const visible = ['web', 'pages'].some((view) =>
            data[view]?.status === 'error'
        ) && data.build_id !== state.errorId;
        if (visible) state.errorId = data.build_id;
        if (groups.length) showOverlay(groups, visible);
        else hideOverlay();
        renderWarnings();
        renderToggle();
    }

    // Adds a link, retaining the old stylesheet until its replacement loads.
    const swapLink = (old, href) => new Promise((resolve) => {
        const link = old.cloneNode();
        link.href = href;
        link.onload = link.onerror = (event) => {
            const ok = event.type === 'load';
            (ok ? old : link).remove();
            if (!ok) log('stylesheet failed:', href);
            resolve(ok);
        };
        old.after(link);
    });

    async function swapCss(id) {
        const theme = document.querySelector('link#writer-theme');
        if (!theme) return fullReload('no #writer-theme link', id);
        const selector = 'link[rel~=stylesheet][href^="/__writer/asset/"]' +
            ':not([href^="/__writer/asset/vendor"])';
        const links = Array.from(document.querySelectorAll(selector));
        const ok = await Promise.all([
            swapLink(theme, '/__writer/theme.css?v=' + id),
            ...links.map((link) => swapLink(
                link, link.getAttribute('href').replace(/\?.*/, '') + '?v=' + id
            ))
        ]);
        log('styles reloaded, build', id);
        return ok.every(Boolean);
    }

    // Theme assets outside the document, excluding scripts and stylesheets.
    const outside = (doc) => Array.from(doc.querySelectorAll(
        '[src^="/__writer/asset/"]:not(script),' +
        'link[href^="/__writer/asset/"]:not([rel~=stylesheet])'
    )).filter((el) => !el.closest('[data-writer-document]'));

    function refreshOutside(next) {
        const now = outside(document);
        const fresh = outside(next);
        if (now.length !== fresh.length) return;
        now.forEach((el, index) => ['src', 'href'].forEach((name) => {
            const url = fresh[index].getAttribute(name);
            if (url !== null && url !== el.getAttribute(name)) {
                el.setAttribute(name, url);
            }
        }));
    }

    function syncAttrs(from, to, keep) {
        Array.from(to.attributes).forEach(({ name, value }) => {
            if (from.getAttribute(name) !== value) from.setAttribute(name, value);
        });
        Array.from(from.attributes).forEach(({ name }) => {
            if (!to.hasAttribute(name) && !keep.includes(name)) {
                from.removeAttribute(name);
            }
        });
    }

    function guard(from, to) {
        if (from.tagName === 'DETAILS') to.toggleAttribute('open', from.open);
        if (from.isEqualNode(to)) return false;
        if (from.dataset.rendered === '1' &&
            from.getAttribute('data-tex') === to.getAttribute('data-tex')) {
            syncAttrs(from, to, ['data-rendered']);
            return false;
        }
        if (from.tagName === 'IMG' &&
            from.getAttribute('src') === to.getAttribute('src')) {
            syncAttrs(from, to, []);
            return false;
        }
        return true;
    }

    function takeAnchor(root) {
        const list = Array.from(root.querySelectorAll('[data-sourcepos]'));
        let anchor = null;
        for (let index = 0; index < list.length; index += 1) {
            const el = list[index];
            const rect = el.getBoundingClientRect();
            if (0 <= rect.top && rect.top < innerHeight) {
                anchor = { el, top: rect.top, index };
                break;
            }
            if (rect.top < 0 && rect.bottom > 0) {
                anchor = { el, top: rect.top, index };
            }
        }
        if (!anchor) return null;
        const el = anchor.el;
        return {
            ...anchor, tag: el.tagName, source: el.getAttribute('data-source'),
            cell: el.getAttribute('data-cell'), text: el.textContent.slice(0, 200)
        };
    }

    function restoreAnchor(anchor) {
        if (!anchor) return;
        const root = document.querySelector('[data-writer-document]');
        const list = Array.from(root.querySelectorAll('[data-sourcepos]'));
        if (!list.length) return;
        let target = null;
        let distance = Infinity;
        list.forEach((el, index) => {
            if (el.tagName === anchor.tag &&
                el.getAttribute('data-source') === anchor.source &&
                el.getAttribute('data-cell') === anchor.cell &&
                el.textContent.slice(0, 200) === anchor.text &&
                Math.abs(index - anchor.index) < distance) {
                target = el;
                distance = Math.abs(index - anchor.index);
            }
        });
        if (!target) {
            target = root.contains(anchor.el) ? anchor.el :
                list[Math.min(anchor.index, list.length - 1)];
        }
        scrollBy(0, target.getBoundingClientRect().top - anchor.top);
    }

    function scrollToChanged(root, changed) {
        if (!changed) return;
        const source = String(changed.source);
        const list = Array.from(root.querySelectorAll('[data-source]'))
            .filter((el) => el.getAttribute('data-source') === source);
        let target = null;
        if (changed.cell != null) {
            target = list.find((el) =>
                el.getAttribute('data-cell') === String(changed.cell)
            );
        } else if (changed.line != null) {
            let width = Infinity;
            let preceding = null;
            let precedingStart = -Infinity;
            list.forEach((el) => {
                if (el.hasAttribute('data-cell')) return;
                const range = el.getAttribute('data-sourcepos')
                    ?.match(/^(\d+)-(\d+)$/);
                if (!range) return;
                const start = Number(range[1]);
                const end = Number(range[2]);
                if (start <= changed.line && changed.line <= end &&
                    end - start <= width) {
                    target = el;
                    width = end - start;
                }
                if (start <= changed.line && start >= precedingStart) {
                    preceding = el;
                    precedingStart = start;
                }
            });
            if (!target) target = preceding;
        }
        if (!target) return;
        const rect = target.getBoundingClientRect();
        if (rect.bottom < 0 || rect.top > innerHeight) {
            target.scrollIntoView({ block: 'center' });
        }
    }

    async function swapContent(id, changed) {
        const next = await getDoc();
        if (!next) {
            state.lastStatus = 'stale';
            log('GET / failed, update postponed');
            return false;
        }
        const fresh = next.querySelector('[data-writer-document]');
        const root = document.querySelector('[data-writer-document]');
        if (!fresh || !root || typeof window.morphdom !== 'function') {
            return fullReload('cannot update in place', id);
        }
        const sig = signature(next);
        if (sig !== baseline || fresh.querySelector('script')) {
            return fullReload('page changed outside the document', id);
        }
        const anchor = takeAnchor(root);
        try {
            window.morphdom(root, fresh, {
                childrenOnly: true, onBeforeElUpdated: guard
            });
            syncAttrs(root, fresh, []);
            if (window.katex) window.__writerRenderMath?.(root);
        } catch (error) {
            return fullReload('in-place update failed: ' + error, id);
        }
        restoreAnchor(anchor);
        scrollToChanged(root, changed);
        refreshOutside(next);
        if (document.title !== next.title) document.title = next.title;
        baseline = sig;
        log('document updated, build', id);
        return true;
    }

    async function apply(data) {
        renderStatus(data);
        if (state.view === 'web') {
            if (data.web.status === 'ok') {
                if ((data.kind !== 'css' || state.lastStatus !== 'ok') &&
                    !(await swapContent(data.build_id, data.changed))) return;
                if (!(await swapCss(data.build_id))) return;
                state.lastStatus = 'ok';
            } else if (data.web.status === 'error') {
                state.lastStatus = 'error';
            } else if (state.lastStatus === 'stale') {
                return;
            }
        } else if (data.pages.status !== 'skipped') {
            if (window.__writerPages) {
                await window.__writerPages.apply(data.pages, data.build_id);
            } else {
                log('pages client is not available');
            }
        }
        store(data.build_id);
    }

    function onHello(data) {
        const again = state.buildId !== null;
        let prev = state.buildId;
        try {
            prev = prev ?? sessionStorage.getItem(KEY);
        } catch { /* storage disabled */ }
        state.retryMs = 500;
        state.connected = true;
        if (prev !== null && +prev !== data.build_id && !again) {
            return fullReload('build ' + prev + ' -> ' + data.build_id, data.build_id);
        }
        log('connected, build', data.build_id);
        if (again) {
            onBuild({ ...data, kind: 'content', changed: null });
            return;
        }
        store(data.build_id);
        renderStatus(data);
        if (state.view === 'web') {
            if (data.web.status === 'error') state.lastStatus = 'error';
            const version = ver(document)?.match(/[?&]v=([^&]*)/)?.[1];
            if (data.web.status === 'ok' && version !== String(data.build_id)) {
                onBuild({ ...data, kind: 'content', changed: null });
            }
        }
    }

    function onBuild(data) {
        log(
            'build', data.build_id,
            data.kind ? data.status + '/' + data.kind : data.status
        );
        queue = queue.then(() => apply(data)).catch((error) =>
            fullReload('update failed: ' + error, data.build_id)
        );
    }

    // Never throws out of an event handler.
    const handler = (fn) => (event) => {
        try {
            const data = JSON.parse(event.data);
            if (typeof data?.build_id === 'number') fn(data);
        } catch (error) {
            log('bad event', error);
        }
    };

    function disconnect() {
        clearTimeout(timer);
        if (source) source.close();
        source = null;
        timer = null;
        state.connected = false;
    }

    function connect() {
        disconnect();
        const es = source = new EventSource('/__writer/events');
        es.addEventListener('hello', handler(onHello));
        es.addEventListener('build', handler(onBuild));
        es.addEventListener('error', () => {
            if (source !== es) return;
            disconnect();
            const ms = state.retryMs;
            state.retryMs = Math.min(ms * 2, 5000);
            log('connection lost, retrying in', ms, 'ms');
            timer = setTimeout(connect, ms);
        });
    }

    function setView(view) {
        try {
            sessionStorage.setItem('writer:view', view);
        } catch { /* storage disabled */ }
        location.assign('/?view=' + view);
    }

    function configureView() {
        const view = new URLSearchParams(location.search).get('view');
        try {
            if (view !== null) {
                sessionStorage.setItem('writer:view', view);
            } else if (location.pathname === '/') {
                const stored = sessionStorage.getItem('writer:view');
                if ((stored === 'web' || stored === 'pages') && stored !== state.view) {
                    location.replace('/?view=' + stored);
                    return false;
                }
            }
        } catch { /* storage disabled */ }
        return true;
    }

    window.__writerLiveReload = { connect, disconnect, state, setView };

    function boot() {
        state.view = document.querySelector('[data-writer-pages]') ? 'pages' : 'web';
        if (state.view === 'web') {
            document.documentElement.style.overflowAnchor = 'none';
        }
        if (!configureView()) return;
        queue = getDoc().then((doc) => {
            if (doc && ver(doc) === ver(document)) baseline = signature(doc);
        });
        renderToggle();
        renderWarnings();
        document.addEventListener('keydown', (event) => {
            const target = event.target;
            if (event.key.toLowerCase() !== 'p' || event.ctrlKey ||
                event.metaKey || event.altKey || event.defaultPrevented ||
                target?.isContentEditable ||
                target?.closest?.('input,textarea,select')) return;
            event.preventDefault();
            setView(otherView());
        });
        connect();
    }

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', boot, { once: true });
    } else {
        boot();
    }
})();

// Compiler-backed pages view (docs/spec/live-reload.md §4.3); ES2020, no deps.
(() => {
    'use strict';

    const LEVELS = [.5, .67, .75, .9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3];
    const KEY = 'writer:pages-zoom';
    const PT_TO_PX = 4 / 3;
    const state = {
        buildId: null, status: 'ok', count: 0,
        hashes: [], sizePt: [], zoom: 'fit', scale: 1
    };
    const log = (...args) => console.debug('[writer]', ...args);
    let root = null;
    let pages = [];
    let publicRevision = 0;
    let queued = null;
    let counterFrame = null;

    function safely(action) {
        try {
            return action();
        } catch (error) {
            log('pages update failed:', error);
        }
    }

    function readZoom() {
        try {
            const stored = sessionStorage.getItem(KEY);
            const number = Number(stored);
            return LEVELS.includes(number) ? number : 'fit';
        } catch {
            return 'fit';
        }
    }

    function saveZoom() {
        try {
            sessionStorage.setItem(KEY, String(state.zoom));
        } catch { /* storage disabled */ }
    }

    function captureAnchor() {
        for (let index = 0; index < pages.length; index += 1) {
            const rect = pages[index].node.getBoundingClientRect();
            if (rect.bottom > 0 && rect.height > 0) {
                return { index, fraction: -rect.top / rect.height };
            }
        }
        return null;
    }

    function restoreAnchor(anchor) {
        if (!anchor || !pages.length) return;
        const index = Math.min(anchor.index, pages.length - 1);
        const rect = pages[index].node.getBoundingClientRect();
        const delta = rect.top + anchor.fraction * rect.height;
        if (Number.isFinite(delta) && Math.abs(delta) > .01) {
            window.scrollBy(0, delta);
        }
    }

    function updateCounter() {
        const counter = document.getElementById('writer-pages-counter');
        if (!counter) return;
        const anchor = captureAnchor();
        const index = pages.length ? (anchor?.index ?? pages.length - 1) + 1 : 0;
        counter.textContent = `Page ${index} / ${pages.length}`;
    }

    function scheduleCounter() {
        if (counterFrame !== null) return;
        counterFrame = requestAnimationFrame(() => {
            counterFrame = null;
            safely(updateCounter);
        });
    }

    function fittedScale() {
        const widest = pages.reduce(
            (maximum, page) => Math.max(maximum, page.width), 0
        );
        if (!(widest > 0)) return 1;
        const width = root.clientWidth || document.documentElement.clientWidth;
        const fit = (width - 32) / widest;
        return Number.isFinite(fit) ? Math.min(2, Math.max(.25, fit)) : 1;
    }

    function sizeImage(image, page) {
        image.width = Math.round(page.width);
        image.height = Math.round(page.height);
        image.style.width = `${page.width * state.scale}px`;
        image.style.height = `${page.height * state.scale}px`;
    }

    function layout() {
        state.scale = state.zoom === 'fit' ? fittedScale() : state.zoom;
        for (const page of pages) {
            page.node.style.width = `${page.width * state.scale}px`;
            page.node.style.height = `${page.height * state.scale}px`;
            sizeImage(page.image, page);
            if (page.pending) sizeImage(page.pending, page);
        }
        const label = document.getElementById('writer-pages-zoom');
        if (label) {
            label.textContent = state.zoom === 'fit'
                ? 'Fit' : `${Math.round(state.scale * 100)}%`;
        }
    }

    function cancelPending(page) {
        if (!page.pending) return;
        page.pending.onload = null;
        page.pending.onerror = null;
        page.pending.remove();
        page.pending = null;
    }

    function newImage(index) {
        const image = document.createElement('img');
        image.alt = `Page ${index + 1}`;
        image.loading = 'lazy';
        image.decoding = 'async';
        image.draggable = false;
        return image;
    }

    function newPage(index) {
        const node = document.createElement('div');
        node.className = 'writer-page';
        node.dataset.page = String(index + 1);
        node.style.position = 'relative';
        const image = newImage(index);
        node.append(image);
        root.append(node);
        return {
            node, image, pending: null, hash: null, imageHash: null,
            width: 0, height: 0
        };
    }

    function showRevision(page, index, hash) {
        if (page.hash === hash) return;
        page.hash = hash;
        page.node.dataset.hash = hash;
        cancelPending(page);
        // A revision may return to the image already on screen while a newer
        // candidate is loading. Keep that image without requesting it again.
        if (page.imageHash === hash) return;
        const source = `/__writer/pages/${index + 1}.svg?v=${hash}`;
        const old = page.image;
        if (!(old.complete && old.naturalWidth > 0)) {
            old.onload = null;
            old.onerror = null;
            const image = newImage(index);
            sizeImage(image, page);
            image.onerror = () => log('page image failed to load:', source);
            old.replaceWith(image);
            page.image = image;
            page.imageHash = hash;
            image.src = source;
            return;
        }

        // The candidate stays in normal layout and uses native lazy loading.
        // A detached Image would eagerly fetch even a far, offscreen revision.
        const candidate = newImage(index);
        sizeImage(candidate, page);
        candidate.style.opacity = '0';
        old.style.position = 'absolute';
        old.style.inset = '0';
        page.pending = candidate;
        page.node.append(candidate);
        const current = () => page.pending === candidate && page.hash === hash
            && page.node.isConnected;
        const reveal = () => safely(() => {
            if (!current()) return;
            old.remove();
            candidate.style.opacity = '';
            page.image = candidate;
            page.imageHash = hash;
            page.pending = null;
        });
        candidate.onload = () => safely(() => {
            if (!current()) return;
            const decoded = candidate.decode
                ? candidate.decode() : Promise.resolve();
            Promise.resolve(decoded).then(reveal, (error) => {
                if (current()) log('page image failed to decode:', source, error);
            });
        });
        candidate.onerror = () => {
            if (current()) log('page image failed to load:', source);
        };
        candidate.src = source;
    }

    function validated(input) {
        if (!input || !Number.isSafeInteger(input.count) || input.count < 0
            || !Array.isArray(input.hashes) || !Array.isArray(input.size_pt)
            || input.hashes.length < input.count
            || input.size_pt.length < input.count) {
            throw new Error('invalid pages state');
        }
        const result = [];
        for (let index = 0; index < input.count; index += 1) {
            const hash = input.hashes[index];
            const size = input.size_pt[index];
            if (typeof hash !== 'string' || !/^[0-9a-f]{16}$/.test(hash)
                || !Array.isArray(size) || size.length !== 2
                || !size.every((number) => Number.isFinite(number) && number > 0)) {
                throw new Error(`invalid page ${index + 1}`);
            }
            result.push({ hash, width: size[0] * PT_TO_PX,
                height: size[1] * PT_TO_PX, size: [...size] });
        }
        return result;
    }

    function updateStatus() {
        const status = document.getElementById('writer-pages-status');
        if (!status) return;
        status.hidden = state.status !== 'error';
        status.textContent = state.status === 'error'
            ? 'pages: build error, ' + (pages.length
                ? 'showing the last good pages' : 'no pages yet') : '';
    }

    function render(input, buildId) {
        if (!root) {
            queued = { input, buildId };
            return;
        }
        const next = validated(input);
        const anchor = captureAnchor();
        root.querySelector('.writer-pages-empty')?.remove();
        // Create and size every page before setting any source, so native lazy
        // loading sees final page positions instead of a zero-height stack.
        for (let index = 0; index < next.length; index += 1) {
            if (!pages[index]) pages.push(newPage(index));
            pages[index].width = next[index].width;
            pages[index].height = next[index].height;
        }
        while (pages.length > next.length) {
            const page = pages.pop();
            cancelPending(page);
            page.node.remove();
        }
        if (!next.length) {
            const empty = document.createElement('p');
            empty.className = 'writer-pages-empty';
            empty.textContent = 'No pages yet.';
            root.append(empty);
        }
        state.buildId = buildId;
        state.status = input.status;
        state.count = next.length;
        state.hashes = next.map((page) => page.hash);
        state.sizePt = next.map((page) => page.size);
        layout();
        restoreAnchor(anchor);
        next.forEach((page, index) => showRevision(pages[index], index, page.hash));
        updateStatus();
        updateCounter();
        log('pages updated, build', buildId);
    }

    function apply(input, buildId) {
        publicRevision += 1;
        safely(() => render(input, buildId));
    }

    function changeZoom(action) {
        let choice = 'fit';
        if (action === 'in') {
            choice = LEVELS.find((level) => level > state.scale + 1e-9);
        } else if (action === 'out') {
            choice = [...LEVELS].reverse().find(
                (level) => level < state.scale - 1e-9
            );
        } else if (action !== 'fit') {
            return;
        }
        if (choice === undefined) return;
        const anchor = captureAnchor();
        state.zoom = choice;
        layout();
        restoreAnchor(anchor);
        updateCounter();
        saveZoom();
    }

    function onKey(event) {
        const target = event.target;
        if (event.ctrlKey || event.metaKey || event.altKey || event.defaultPrevented
            || target?.isContentEditable
            || target?.closest?.('input,textarea,select')) return;
        const action = { '+': 'in', '=': 'in', '-': 'out', '0': 'fit' }[event.key];
        if (!action) return;
        event.preventDefault();
        safely(() => changeZoom(action));
    }

    function boot() {
        root = document.getElementById('writer-pages');
        if (!root) {
            log('pages container is not available');
            return;
        }
        document.documentElement.style.overflowAnchor = 'none';
        root.style.overflowAnchor = 'none';
        state.zoom = readZoom();
        const toolbar = document.getElementById('writer-pages-toolbar');
        toolbar?.addEventListener('click', (event) => {
            const button = event.target?.closest?.('button[data-zoom]');
            if (button) safely(() => changeZoom(button.dataset.zoom));
        });
        document.addEventListener('keydown', onKey);
        window.addEventListener(
            'scroll', () => safely(scheduleCounter), { passive: true }
        );
        window.addEventListener('resize', () => safely(() => {
            if (state.zoom === 'fit') {
                const anchor = captureAnchor();
                layout();
                restoreAnchor(anchor);
            }
            updateCounter();
        }));
        if (queued) {
            const { input, buildId } = queued;
            queued = null;
            safely(() => render(input, buildId));
        }
        const started = publicRevision;
        safely(() => {
            fetch('/__writer/state', { cache: 'no-store' })
                .then((response) => {
                    if (!response.ok) {
                        throw new Error(`state HTTP ${response.status}`);
                    }
                    return response.json();
                })
                .then((data) => {
                    if (publicRevision !== started) return;
                    safely(() => render(data.pages, data.build_id));
                })
                .catch((error) => log('pages state fetch failed:', error));
        });
    }

    window.__writerPages = { apply, state };
    if (document.getElementById('writer-pages')) {
        safely(boot);
    } else if (document.readyState === 'loading') {
        document.addEventListener(
            'DOMContentLoaded', () => safely(boot), { once: true }
        );
    } else {
        safely(boot);
    }
})();

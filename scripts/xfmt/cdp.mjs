// One headless Chrome driven over the DevTools protocol, for the xfmt
// harness (docs/dev/xfmt-harness.md).
//
// Chrome runs in its own process group with its own profile directory,
// whose name starts with "xfmt-chrome-". `withChrome` closes Chrome and
// removes the profile on every exit path: a normal return, a thrown error,
// SIGINT, SIGTERM and SIGHUP, and process exit.
import { spawn } from "node:child_process";
import {
    existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync
} from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";

export const PROFILE_PREFIX = "xfmt-chrome-";

const CHROME_APPS = [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
];

// Chrome's command line before the profile and the page, as
// src/pdf/devtools.rs starts it, with a port instead of a pipe.
const FLAGS = [
    "--headless=new", "--disable-gpu", "--no-first-run",
    "--no-default-browser-check", "--disable-extensions",
    "--disable-background-networking", "--disable-component-update",
    "--disable-sync", "--disable-default-apps",
    "--disable-domain-reliability",
    "--disable-client-side-phishing-detection", "--no-service-autorun",
    "--password-store=basic", "--use-mock-keychain", "--hide-scrollbars",
    "--mute-audio", "--remote-debugging-port=0",
];

const sleep = ms => new Promise(done => setTimeout(done, ms));

// The browser to start: CHROME_PATH, else the first macOS app found.
export function findChrome() {
    if (process.env.CHROME_PATH) {
        return process.env.CHROME_PATH;
    }
    const found = CHROME_APPS.find(path => existsSync(path));
    if (!found) {
        throw new Error("no Chrome found; set CHROME_PATH");
    }
    return found;
}

// A fresh profile directory: `wanted` when given (it must be empty or
// missing and its name must start with PROFILE_PREFIX), else a new one in
// the temporary directory.
function makeProfile(wanted) {
    if (!wanted) {
        return mkdtempSync(join(tmpdir(), PROFILE_PREFIX));
    }
    if (!basename(wanted).startsWith(PROFILE_PREFIX)) {
        throw new Error(`profile name must start with ${PROFILE_PREFIX}`);
    }
    mkdirSync(wanted, { recursive: true });
    return wanted;
}

// Starts Chrome, opens one page and calls `body(page)`, where `page` has
// `send(method, params, timeout)`, `once(method, timeout)` and `browser`
// (the same calls on the browser target). Chrome is closed and its
// profile removed however `body` ends.
export async function withChrome(options, body) {
    // Find the browser first: a missing one then leaves no profile behind.
    const chrome = findChrome();
    const profile = makeProfile(options.profile);
    const proc = spawn(chrome, [
        ...FLAGS, `--user-data-dir=${profile}`, "about:blank"
    ], { stdio: "ignore", detached: true });
    let exited = false;
    const exit = new Promise(done => proc.once("exit", () => {
        exited = true;
        done();
    }));
    proc.once("error", () => { exited = true; });

    // Synchronous last resort for process exit and signals.
    const killNow = () => {
        if (!exited) {
            try {
                process.kill(-proc.pid, "SIGKILL");
            } catch {
                // Already gone.
            }
        }
        rmSync(profile, { recursive: true, force: true });
    };
    const onSignal = signal => {
        killNow();
        process.exit(signal === "SIGINT" ? 130 : 143);
    };
    process.once("exit", killNow);
    for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
        process.once(signal, onSignal);
    }

    let socket = null;
    try {
        socket = await connect(profile, () => exited, options.timeout);
        const client = new Client(socket, options.timeout ?? 30000);
        const { targetInfos } = await client.send("Target.getTargets");
        let target = targetInfos.find(info => info.type === "page");
        if (!target) {
            target = await client.send(
                "Target.createTarget", { url: "about:blank" }
            );
        }
        const { sessionId } = await client.send("Target.attachToTarget", {
            targetId: target.targetId, flatten: true
        });
        const page = client.session(sessionId);
        page.browser = client;
        return await body(page);
    } finally {
        await close(socket, proc, exit, () => exited);
        killNow();
        await removeProfile(profile);
        process.removeListener("exit", killNow);
        for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
            process.removeListener(signal, onSignal);
        }
    }
}

// Waits for Chrome's DevTools port and opens the browser's socket.
async function connect(profile, hasExited, timeout) {
    const portFile = join(profile, "DevToolsActivePort");
    const deadline = Date.now() + (timeout ?? 30000);
    let lines = null;
    while (lines === null) {
        if (hasExited()) {
            throw new Error("Chrome exited before it opened a DevTools port");
        }
        if (Date.now() > deadline) {
            throw new Error("Chrome did not open a DevTools port in time");
        }
        if (existsSync(portFile)) {
            const text = readFileSync(portFile, "utf8").split("\n");
            if (text.length >= 2 && text[1]) {
                lines = text;
                break;
            }
        }
        await sleep(50);
    }
    const url = `ws://127.0.0.1:${lines[0].trim()}${lines[1].trim()}`;
    const socket = new WebSocket(url);
    await new Promise((done, fail) => {
        socket.addEventListener("open", done, { once: true });
        socket.addEventListener("error", () => fail(new Error(
            `cannot connect to ${url}`
        )), { once: true });
    });
    return socket;
}

// Asks Chrome to close, then kills its process group if it is still there.
async function close(socket, proc, exit, hasExited) {
    if (socket && socket.readyState === WebSocket.OPEN && !hasExited()) {
        try {
            socket.send(JSON.stringify({ id: 1e9, method: "Browser.close" }));
        } catch {
            // The socket closed meanwhile; the kill below covers it.
        }
        await Promise.race([exit, sleep(3000)]);
    }
    if (socket) {
        socket.close();
    }
    for (const signal of ["SIGTERM", "SIGKILL"]) {
        try {
            process.kill(-proc.pid, signal);
        } catch {
            // The group is gone.
        }
        await Promise.race([exit, sleep(2000)]);
    }
}

// Removes the profile, retrying while Chrome's last writes settle.
async function removeProfile(profile) {
    for (let tries = 0; tries < 20 && existsSync(profile); tries++) {
        try {
            rmSync(profile, { recursive: true, force: true });
        } catch {
            await sleep(100);
        }
    }
}

// The DevTools connection: commands by id, events by name and session.
class Client {
    constructor(socket, timeout) {
        this.socket = socket;
        this.timeout = timeout;
        this.nextId = 1;
        this.pending = new Map();
        this.waiters = [];
        socket.addEventListener("message", message => {
            this.receive(JSON.parse(message.data));
        });
        socket.addEventListener("close", () => {
            const error = new Error("the DevTools connection closed");
            for (const { fail } of this.pending.values()) {
                fail(error);
            }
            this.pending.clear();
        });
    }

    receive(data) {
        if (data.id !== undefined && this.pending.has(data.id)) {
            const { done, fail, method } = this.pending.get(data.id);
            this.pending.delete(data.id);
            if (data.error) {
                fail(new Error(`${method}: ${JSON.stringify(data.error)}`));
            } else {
                done(data.result);
            }
            return;
        }
        if (!data.method) {
            return;
        }
        this.waiters = this.waiters.filter(waiter => {
            if (waiter.method !== data.method) {
                return true;
            }
            if (waiter.sessionId !== (data.sessionId ?? null)) {
                return true;
            }
            waiter.done(data.params);
            return false;
        });
    }

    send(method, params = {}, sessionId = null, timeout = this.timeout) {
        const id = this.nextId++;
        const message = { id, method, params };
        if (sessionId) {
            message.sessionId = sessionId;
        }
        return new Promise((done, fail) => {
            const timer = setTimeout(() => {
                this.pending.delete(id);
                fail(new Error(`${method}: no answer in ${timeout} ms`));
            }, timeout);
            // A timer alone does not keep the process alive.
            timer.unref();
            this.pending.set(id, {
                method,
                done: value => {
                    clearTimeout(timer);
                    done(value);
                },
                fail: error => {
                    clearTimeout(timer);
                    fail(error);
                }
            });
            this.socket.send(JSON.stringify(message));
        });
    }

    // A promise for the next event `method` on `sessionId`; register it
    // before the command that causes the event.
    once(method, sessionId = null, timeout = this.timeout) {
        return new Promise((done, fail) => {
            const waiter = { method, sessionId, done: null };
            const timer = setTimeout(() => {
                this.waiters = this.waiters.filter(other => other !== waiter);
                fail(new Error(`${method}: no event in ${timeout} ms`));
            }, timeout);
            timer.unref();
            waiter.done = params => {
                clearTimeout(timer);
                done(params);
            };
            this.waiters.push(waiter);
        });
    }

    session(sessionId) {
        return {
            send: (method, params, timeout) =>
                this.send(method, params, sessionId, timeout),
            once: (method, timeout) => this.once(method, sessionId, timeout),
        };
    }
}

// Loads `url` in `page` and waits for its load event and its fonts.
export async function load(page, url) {
    await page.send("Page.enable");
    const loaded = page.once("Page.loadEventFired");
    const result = await page.send("Page.navigate", { url });
    if (result.errorText) {
        loaded.catch(() => {});
        throw new Error(`cannot load ${url}: ${result.errorText}`);
    }
    await loaded;
    await evaluate(page, "document.fonts.ready.then(() => true)");
}

// Evaluates `expression` in `page` and returns its value.
export async function evaluate(page, expression) {
    const result = await page.send("Runtime.evaluate", {
        expression, awaitPromise: true, returnByValue: true
    });
    if (result.exceptionDetails) {
        const details = result.exceptionDetails;
        const text = details.exception?.description ?? details.text;
        throw new Error(`JavaScript threw: ${text}`);
    }
    return result.result.value;
}

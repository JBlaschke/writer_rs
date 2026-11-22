// Print an HTML file to PDF with headless Chrome over the DevTools protocol
// and report the screen layout's text-line rectangles of the document body.
// usage: node cdp-print.mjs <in.html> <out.pdf> <lines.json>
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const [input, output, linesOut] = process.argv.slice(2);
const chrome = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const profile = mkdtempSync(join(process.env.SCRATCH ?? tmpdir(), "chrome-"));
const proc = spawn(chrome, [
    "--headless=new", "--remote-debugging-port=0", `--user-data-dir=${profile}`,
    "--no-first-run", "--no-default-browser-check", "--disable-gpu",
    "about:blank"
], { stdio: "ignore" });

const sleep = ms => new Promise(done => setTimeout(done, ms));
const portFile = join(profile, "DevToolsActivePort");
let port = null;
for (let tries = 0; tries < 200 && port === null; tries++) {
    if (existsSync(portFile)) {
        port = readFileSync(portFile, "utf8").split("\n")[0];
    } else {
        await sleep(100);
    }
}
if (port === null) {
    proc.kill("SIGKILL");
    throw new Error("Chrome did not open a DevTools port");
}

const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find(target => target.type === "page");
const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise(done => socket.addEventListener("open", done));
let nextId = 1;
const pending = new Map();
const events = [];
socket.addEventListener("message", message => {
    const data = JSON.parse(message.data);
    if (data.id && pending.has(data.id)) {
        pending.get(data.id)(data);
        pending.delete(data.id);
    } else if (data.method) {
        events.push(data.method);
    }
});
const send = (method, params = {}) => new Promise((done, fail) => {
    const id = nextId++;
    pending.set(id, data => data.error ? fail(new Error(
        `${method}: ${JSON.stringify(data.error)}`
    )) : done(data.result));
    socket.send(JSON.stringify({ id, method, params }));
});

try {
    await send("Page.enable");
    await send("Page.navigate", { url: "file://" + resolve(input) });
    for (let tries = 0; tries < 300; tries++) {
        if (events.includes("Page.loadEventFired")) break;
        await sleep(50);
    }
    await send("Runtime.evaluate", {
        expression: "document.fonts.ready.then(() => true)",
        awaitPromise: true
    });

    // Screen layout: one entry per text line in the document body.
    const measure = `(() => {
        const root = document.querySelector("[data-writer-document]");
        const rows = [];
        const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
        while (walker.nextNode()) {
            const node = walker.currentNode;
            if (!node.textContent.trim()) continue;
            const range = document.createRange();
            range.selectNodeContents(node);
            const style = getComputedStyle(node.parentElement);
            for (const rect of range.getClientRects()) {
                rows.push({
                    top: rect.top + scrollY, bottom: rect.bottom + scrollY,
                    size: parseFloat(style.fontSize),
                    text: node.textContent.trim().slice(0, 40)
                });
            }
        }
        return JSON.stringify(rows);
    })()`;
    const screen = await send("Runtime.evaluate", {
        expression: measure, returnByValue: true
    });
    writeFileSync(linesOut, screen.result.value);

    const pdf = await send("Page.printToPDF", {
        printBackground: true, preferCSSPageSize: true,
        displayHeaderFooter: false, scale: 1
    });
    writeFileSync(output, Buffer.from(pdf.data, "base64"));
} finally {
    socket.close();
    proc.kill("SIGKILL");
}

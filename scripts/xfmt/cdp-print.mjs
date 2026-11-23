// Reads the screen layout of HTML files and prints them to PDF, with one
// headless Chrome driven over the DevTools protocol (cdp.mjs).
//
// usage: node cdp-print.mjs <jobs.json | -> [--profile <dir>]
//
// jobs.json (or standard input, for "-") is a list of jobs, each
//   { "html": file, "lines": out.json, "pdf": out.pdf or null,
//     "media": "print" or "screen", "width": px, "height": px,
//     "root": CSS selector }
// For each job the page is loaded with `media` emulated and the viewport
// set to `width` (R13 §0.2), and lines.js's rows are written to `lines`.
// When `pdf` is set, the emulation is then cleared and Page.printToPDF's
// output, always with print media, is written to `pdf`. Prints Chrome's
// version on the first line of its output.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { evaluate, load, withChrome } from "./cdp.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const measure = readFileSync(join(here, "lines.js"), "utf8");

const args = process.argv.slice(2);
const profileAt = args.indexOf("--profile");
const profile = profileAt >= 0 ? args.splice(profileAt, 2)[1] : null;
if (args.length !== 1) {
    console.error(
        "usage: node cdp-print.mjs <jobs.json | -> [--profile <dir>]"
    );
    process.exit(2);
}
const jobs = JSON.parse(readFileSync(args[0] === "-" ? 0 : args[0], "utf8"));

async function run(page, job) {
    await page.send("Emulation.setEmulatedMedia", {
        media: job.media ?? "print"
    });
    await page.send("Emulation.setDeviceMetricsOverride", {
        width: Math.round(job.width ?? 800),
        height: Math.round(job.height ?? 1100),
        deviceScaleFactor: 1, mobile: false
    });
    await load(page, pathToFileURL(resolve(job.html)).href);
    const root = JSON.stringify(job.root ?? "[data-writer-document]");
    const rows = await evaluate(page, `(${measure.trim()})(${root})`);
    writeFileSync(job.lines, JSON.stringify(rows, null, 1));
    if (job.pdf) {
        // Chrome's own print: no viewport override and no emulated media,
        // so that print media applies whatever `media` the job read the
        // screen layout with.
        await page.send("Emulation.clearDeviceMetricsOverride");
        await page.send("Emulation.setEmulatedMedia", { media: "" });
        const printed = await page.send("Page.printToPDF", {
            printBackground: true, preferCSSPageSize: true,
            displayHeaderFooter: false, scale: 1
        }, 120000);
        writeFileSync(job.pdf, Buffer.from(printed.data, "base64"));
    }
}

// A job that fails is reported and the next one runs; the exit status is
// 1 when any failed.
try {
    await withChrome({ profile }, async page => {
        const version = await page.browser.send("Browser.getVersion");
        console.log(`chrome: ${version.product}`);
        for (const job of jobs) {
            try {
                await run(page, job);
            } catch (error) {
                console.error(`cdp-print: ${job.html}: ${error.message}`);
                process.exitCode = 1;
            }
        }
    });
} catch (error) {
    console.error(`cdp-print: ${error.message}`);
    process.exitCode = 1;
}

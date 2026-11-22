// Loads an HTML file in one headless Chrome driven over the DevTools
// protocol (cdp.mjs) and writes the value of a JavaScript expression,
// evaluated in the page, as JSON: for one-off probes such as
// probes/residual/expr.js and probes/center/expr.js.
//
// usage: node cdp-eval.mjs <in.html> <out.json> <expr.js>
//            [--media print|screen] [--width px]
//
// The expression may return a promise. Without --width the viewport is
// Chrome's default; without --media the page's own media (screen) holds.
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { evaluate, load, withChrome } from "./cdp.mjs";

const args = process.argv.slice(2);
const option = name => {
    const at = args.indexOf(name);
    return at >= 0 ? args.splice(at, 2)[1] : null;
};
const media = option("--media");
const width = option("--width");
if (args.length !== 3) {
    console.error(
        "usage: node cdp-eval.mjs <in.html> <out.json> <expr.js>"
        + " [--media print|screen] [--width px]"
    );
    process.exit(2);
}
const [input, output, script] = args;
const expression = readFileSync(script, "utf8");

try {
    await withChrome({}, async page => {
        if (media) {
            await page.send("Emulation.setEmulatedMedia", { media });
        }
        if (width) {
            await page.send("Emulation.setDeviceMetricsOverride", {
                width: Math.round(Number(width)), height: 1100,
                deviceScaleFactor: 1, mobile: false
            });
        }
        await load(page, pathToFileURL(resolve(input)).href);
        const value = await evaluate(page, expression);
        writeFileSync(output, JSON.stringify(value, null, 1));
    });
} catch (error) {
    console.error(`cdp-eval: ${error.message}`);
    process.exitCode = 1;
}

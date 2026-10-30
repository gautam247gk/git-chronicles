// Runs the viewer's inline script against a stub DOM and checks behaviour. Usage: node viewer_smoke.js page.html
const fs = require("fs"), vm = require("vm"), assert = require("assert");
const html = fs.readFileSync(process.argv[2], "utf8");
const data = html.split('<script type="application/json" id="data">')[1].split("</script>")[0];
const main = /<script>([\s\S]*?)<\/script>/.exec(html)[1];

const ctx = new Proxy({}, { get: (t, k) => (k in t ? t[k] : () => {}), set: (t, k, v) => ((t[k] = v), true) });
const el = () => ({ style: {}, dataset: {}, classList: { toggle() {} }, value: "0", textContent: "", innerHTML: "",
  appendChild() {}, addEventListener() {}, focus() {}, blur() {}, setPointerCapture() {},
  getBoundingClientRect: () => ({ left: 0, top: 0, width: 800, height: 600 }), getContext: () => ctx });
const els = {};
const listeners = {}, frames = [];
const sandbox = {
  document: { getElementById: id => els[id] || (els[id] = id === "data" ? { ...el(), textContent: data } : el()),
    querySelectorAll: () => [], createElement: el, body: { dataset: {} } },
  getComputedStyle: () => ({ getPropertyValue: () => "#888" }),
  addEventListener: (t, f) => (listeners[t] = f), devicePixelRatio: 1,
  requestAnimationFrame: f => frames.push(f), console, Math, Map, Set, Array, JSON, String, Number, Object, Date,
};
sandbox.globalThis = sandbox;
vm.runInNewContext(main + `;globalThis.probe = () => ({ cur, n: N, nodes: nodes.length, cycle: [...cycleNodes].sort(), overlay, selected,
  first: toScreen(nodes[0].p), firstId: nodes[0].id, playing })`, sandbox);

const key = (k, extra = {}) => listeners.keydown({ key: k, target: {}, preventDefault() {}, ...extra });
const runFrames = n => { for (let i = 0; i < n; i++) frames.splice(0).forEach(f => f(1000 + i * 16)); };
runFrames(5);

let p = sandbox.probe();
assert.strictEqual(p.cur, p.n - 1, "starts at the last sample");
assert.ok(p.nodes > 0);
assert.strictEqual(JSON.stringify(p.cycle), '["src/a.ts","src/b.ts","src/c.ts"]', "client-side Tarjan finds the cycle");

key("ArrowLeft"); key("h");
p = sandbox.probe();
assert.strictEqual(p.cur, p.n - 3);
assert.strictEqual(JSON.stringify(p.cycle), "[]", "cycle does not exist before it was introduced");
key("l"); key("ArrowRight");
assert.strictEqual(sandbox.probe().cur, p.n - 1);

key("4");
assert.strictEqual(sandbox.probe().overlay, "cycles");
key("2"); key("3"); key("5"); key("1");
assert.strictEqual(sandbox.probe().overlay, "architecture");

key(" ");
assert.strictEqual(sandbox.probe().playing, true);
key(" ");
key("+"); key("-");
key("ArrowLeft", { shiftKey: true });   // tag jump with no tags must not throw

// click the first node: inspector must show it (and be HTML-escaped)
p = sandbox.probe();
els.c.onpointerdown({ clientX: p.first[0], clientY: p.first[1], pointerId: 1 });
els.c.onpointerup({ clientX: p.first[0], clientY: p.first[1] });
p = sandbox.probe();
assert.strictEqual(p.selected, p.firstId);
assert.ok(els.inspector.innerHTML.includes(p.firstId) && els.inspector.innerHTML.includes("Genesis"));
key("f"); key("r"); runFrames(3);
key("Escape");
assert.strictEqual(sandbox.probe().selected, null);
console.log("viewer smoke ok");

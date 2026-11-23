(() => {
  const q = (sel) => document.querySelector(sel);
  const cs = (el) => el ? getComputedStyle(el) : null;
  const out = {};
  for (const [k, sel] of [["subtitle", "p.subtitle"], ["date", "p.date"], ["centerdiv p", "div.center > p"], ["body p", "[data-writer-document] > p"]]) {
    const s = cs(q(sel)); out[k] = s && {textAlign: s.textAlign, marginBottom: s.marginBottom};
  }
  const bq = document.querySelectorAll("blockquote > p");
  out["blockquote last p"] = bq.length ? getComputedStyle(bq[bq.length-1]).marginBottom : null;
  return out;
})()

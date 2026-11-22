(() => Array.from(document.querySelectorAll('#doc > p')).map(p => {
  const r = p.getBoundingClientRect(); const b = p.querySelector('.b').getBoundingClientRect();
  const cs = getComputedStyle(p);
  return {cls: p.className, top: r.top*0.75, h: r.height*0.75, above: (b.top - r.top)*0.75, fw: cs.fontWeight};
}))()

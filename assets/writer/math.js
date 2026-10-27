// KaTeX initialisation for writer, included in every document that uses math.
// Renders every [data-math-style] element ("inline" or "display") with KaTeX.
// window.__writerKatexOptions (set by math_init()) extends the defaults below.
// Rendered elements are marked data-rendered="1", so
// window.__writerRenderMath(root) can be called again on new content (live reload).
(function () {
    function render(root) {
        if (!window.katex) return;
        root.querySelectorAll('[data-math-style]:not([data-rendered])').forEach(function (el) {
            if (el.dataset.rendered) return;
            var display = el.dataset.mathStyle === 'display';
            try {
                katex.render(
                    el.textContent, el,
                    Object.assign(
                        { displayMode: display, throwOnError: false },
                        window.__writerKatexOptions || {}
                    )
                );
                el.dataset.rendered = '1';
            } catch (e) {
                console.warn('[writer] katex', e);
            }
        });
    }
    window.__writerRenderMath = render;
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', function () { render(document); });
    } else {
        render(document);
    }
})();

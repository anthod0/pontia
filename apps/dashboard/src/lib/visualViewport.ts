export function installVisualViewportCssVars(win: Window = window): () => void {
  const root = win.document.documentElement;
  const viewport = win.visualViewport;

  function update(): void {
    const top = viewport?.offsetTop ?? 0;
    const bottom = viewport ? Math.max(0, win.innerHeight - viewport.height - viewport.offsetTop) : 0;
    root.style.setProperty('--visual-viewport-top', `${Math.round(top)}px`);
    root.style.setProperty('--visual-viewport-bottom', `${Math.round(bottom)}px`);
  }

  update();

  if (!viewport) {
    return () => {
      root.style.setProperty('--visual-viewport-top', '0px');
      root.style.setProperty('--visual-viewport-bottom', '0px');
    };
  }

  viewport.addEventListener('resize', update);
  viewport.addEventListener('scroll', update);

  return () => {
    viewport.removeEventListener('resize', update);
    viewport.removeEventListener('scroll', update);
    root.style.setProperty('--visual-viewport-top', '0px');
    root.style.setProperty('--visual-viewport-bottom', '0px');
  };
}

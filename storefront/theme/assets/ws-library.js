(() => {
  'use strict';
  if (window.wsLibraryInitialized) return;
  window.wsLibraryInitialized = true;
  const instances = new Map();
  const init = scope => {
    const roots = [...(scope.matches?.('[data-ws-library]') ? [scope] : []), ...scope.querySelectorAll('[data-ws-library]')];
    roots.forEach(root => {
      if (instances.has(root)) return;
      const events = new AbortController();
      const on = (target, type, handler, options = {}) => target?.addEventListener(type, handler, {...options, signal: events.signal});
      const dialog = root.querySelector('[data-ws-library-dialog]');
      const image = root.querySelector('[data-ws-library-image]');
      const caption = root.querySelector('[data-ws-library-caption]');
      let opener = null;
      let sync = () => {};
      root.querySelectorAll('[data-ws-artwork]').forEach(button => on(button, 'click', () => {
        if (!dialog || !image) return;
        opener = button;
        image.src = button.dataset.full;
        image.alt = button.dataset.title;
        if (caption) caption.textContent = button.dataset.title + ' · 1536 × 969';
        dialog.showModal();
        dialog.querySelector('button')?.focus();
        sync();
      }));
      on(root.querySelector('[data-ws-library-close]'), 'click', () => dialog.close());
      on(dialog, 'click', event => { if (event.target === dialog) dialog.close(); });
      on(dialog, 'close', () => {
        image?.removeAttribute('src');
        opener?.focus({preventScroll: true});
        sync();
      });
      let stop = () => {};
      let resizeObserver = null;
      const destroy = () => {
        stop();
        events.abort();
        resizeObserver?.disconnect();
        if (dialog?.open) dialog.close();
        image?.removeAttribute('src');
        instances.delete(root);
      };
      instances.set(root, {destroy, sync: () => sync(), stop: () => stop()});
      const track = root.querySelector('[data-ws-library-track]');
      if (!track) return;
      const cards = [...track.children];
      if (cards.length < 2) return;
      let hovered = false;
      let raf = null, lastTime = null, pitch = 0, offset = 0, head = 0, index = 0;
      const speed = 34; // CSS pixels per second, independent of refresh rate.
      stop = () => {
        if (raf !== null) cancelAnimationFrame(raf);
        raf = null;
        lastTime = null;
      };
      const updateCounter = () => {
        index = (head + (pitch ? Math.floor((offset + pitch / 2) / pitch) : 0)) % cards.length;
      };
      const moveFirst = () => {
        const first = track.firstElementChild;
        if (track.moveBefore) track.moveBefore(first, null);
        else {
          const active = document.activeElement;
          track.appendChild(first);
          if (first.contains(active) && document.activeElement !== active) active.focus({preventScroll: true});
        }
      };
      const canRun = () => !document.hidden && !hovered && pitch > 0;
      const normalize = () => {
        // Recycle the same eight originals; no duplicate controls, IDs or ARIA.
        // DOM movement and scroll compensation happen in one paint frame.
        while (pitch > 0 && offset >= pitch) {
          moveFirst();
          offset -= pitch;
          head = (head + 1) % cards.length;
        }
        track.scrollLeft = offset;
        updateCounter();
      };
      const frame = time => {
        raf = null;
        if (!canRun()) { lastTime = null; return; }
        if (lastTime !== null) offset += Math.min(64, Math.max(0, time - lastTime)) * speed / 1000;
        lastTime = time;
        normalize();
        raf = requestAnimationFrame(frame);
      };
      sync = () => {
        if (!canRun()) { stop(); return; }
        if (raf === null) {
          offset = Math.max(0, track.scrollLeft);
          normalize();
          raf = requestAnimationFrame(frame);
        }
      };
      const measure = () => {
        const first = track.firstElementChild;
        const second = first?.nextElementSibling;
        const nextPitch = second ? second.getBoundingClientRect().left - first.getBoundingClientRect().left : 0;
        if (nextPitch > 0) {
          offset = pitch ? offset / pitch * nextPitch : Math.max(0, track.scrollLeft);
          pitch = nextPitch;
          normalize();
        }
        sync();
      };
      const go = (next, focusCard = false) => {
        index = (next + cards.length) % cards.length;
        const target = cards[index];
        while (track.firstElementChild !== target) moveFirst();
        head = index;
        offset = 0;
        track.scrollLeft = 0;
        updateCounter();
        if (focusCard) target.querySelector('[data-ws-artwork]')?.focus({preventScroll: true});
        sync();
      };
      on(track, 'keydown', event => {
        if (event.key !== 'ArrowRight' && event.key !== 'ArrowLeft') return;
        event.preventDefault();
        go(index + (event.key === 'ArrowRight' ? 1 : -1), event.target !== track);
      });
      on(track, 'pointerenter', event => { if (event.pointerType === 'mouse') { hovered = true; stop(); } });
      on(track, 'pointerleave', event => { if (event.pointerType === 'mouse') { hovered = false; sync(); } });
      on(track, 'scroll', () => {
        // Keep subpixel RAF progress instead of re-reading rounded programmatic
        // scroll values each frame. Mouse hover allows native scroll adjustment.
        if (raf === null) offset = Math.max(0, track.scrollLeft);
        updateCounter();
      }, {passive: true});
      on(document, 'visibilitychange', sync);
      if (window.ResizeObserver) {
        resizeObserver = new ResizeObserver(measure);
        resizeObserver.observe(track);
      } else on(window, 'resize', measure);
      measure();
    });
  };
  init(document);
  document.addEventListener('shopify:section:load', event => init(event.target));
  document.addEventListener('shopify:section:unload', event => {
    instances.forEach((instance, root) => { if (event.target === root || event.target.contains(root)) instance.destroy(); });
  });
  window.addEventListener('pagehide', () => instances.forEach(instance => instance.stop()));
  window.addEventListener('pageshow', () => instances.forEach(instance => instance.sync()));
})();

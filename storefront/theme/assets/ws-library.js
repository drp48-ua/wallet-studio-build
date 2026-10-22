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
      let resizeObserver = null, visibilityObserver = null, focusTimer = null;
      const destroy = () => {
        stop();
        clearTimeout(focusTimer);
        events.abort();
        resizeObserver?.disconnect();
        visibilityObserver?.disconnect();
        if (dialog?.open) dialog.close();
        image?.removeAttribute('src');
        instances.delete(root);
      };
      instances.set(root, {destroy, sync: () => sync(), stop: () => stop()});
      const track = root.querySelector('[data-ws-library-track]');
      if (!track) return;
      const cards = [...track.children];
      if (cards.length < 2) return;
      const toggle = root.querySelector('[data-ws-library-toggle]');
      const counter = root.querySelector('[data-ws-library-counter]');
      const motion = matchMedia('(prefers-reduced-motion: reduce)');
      let paused = motion.matches, hovered = false, focused = false, visible = true;
      let raf = null, lastTime = null, pitch = 0, offset = 0, head = 0, index = 0;
      const speed = 34; // CSS pixels per second, independent of refresh rate.
      stop = () => {
        if (raf !== null) cancelAnimationFrame(raf);
        raf = null;
        lastTime = null;
      };
      const updateCounter = () => {
        index = (head + (pitch ? Math.floor((offset + pitch / 2) / pitch) : 0)) % cards.length;
        const label = (index + 1) + ' / ' + cards.length;
        if (counter && counter.textContent !== label) counter.textContent = label;
      };
      const updateToggle = () => {
        if (!toggle) return;
        toggle.textContent = paused ? toggle.dataset.play : toggle.dataset.pause;
        toggle.setAttribute('aria-pressed', String(paused));
        toggle.disabled = motion.matches;
      };
      const canRun = () => !paused && !motion.matches && !document.hidden && visible && !hovered && !focused && !dialog?.open && pitch > 0;
      const normalize = () => {
        // Recycle the same eight originals; no duplicate controls, IDs or ARIA.
        // DOM movement and scroll compensation happen in one paint frame.
        while (pitch > 0 && offset >= pitch) {
          track.appendChild(track.firstElementChild);
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
      const pause = () => { paused = true; updateToggle(); stop(); };
      const go = (next, focusCard = false) => {
        pause();
        index = (next + cards.length) % cards.length;
        const target = cards[index];
        while (track.firstElementChild !== target) track.appendChild(track.firstElementChild);
        head = index;
        offset = 0;
        track.scrollLeft = 0;
        updateCounter();
        if (focusCard) target.querySelector('[data-ws-artwork]')?.focus({preventScroll: true});
      };
      on(toggle, 'click', () => {
        paused = !paused;
        // An explicit keyboard Play may resume while its own control has focus.
        if (!paused && document.activeElement === toggle) focused = false;
        updateToggle(); sync();
      });
      on(root.querySelector('[data-ws-library-prev]'), 'click', () => go(index - 1));
      on(root.querySelector('[data-ws-library-next]'), 'click', () => go(index + 1));
      on(track, 'keydown', event => {
        if (event.key !== 'ArrowRight' && event.key !== 'ArrowLeft') return;
        event.preventDefault();
        go(index + (event.key === 'ArrowRight' ? 1 : -1), event.target !== track);
      });
      on(track, 'pointerenter', () => { hovered = true; stop(); });
      on(track, 'pointerleave', () => { hovered = false; sync(); });
      on(track, 'pointerdown', pause);
      on(track, 'touchstart', pause, {passive: true});
      on(track, 'wheel', pause, {passive: true});
      on(root, 'focusin', () => { focused = true; stop(); });
      on(root, 'focusout', () => {
        clearTimeout(focusTimer);
        focusTimer = setTimeout(() => { focused = root.contains(document.activeElement); sync(); }, 0);
      });
      on(track, 'scroll', () => {
        // Keep subpixel RAF progress instead of re-reading rounded programmatic
        // scroll values each frame. Native interaction pauses before scrolling.
        if (raf === null) offset = Math.max(0, track.scrollLeft);
        updateCounter();
      }, {passive: true});
      on(document, 'visibilitychange', sync);
      on(motion, 'change', () => { if (motion.matches) paused = true; updateToggle(); sync(); });
      if (window.ResizeObserver) {
        resizeObserver = new ResizeObserver(measure);
        resizeObserver.observe(track);
      } else on(window, 'resize', measure);
      if (window.IntersectionObserver) {
        visibilityObserver = new IntersectionObserver(entries => { visible = entries[0].isIntersecting; sync(); });
        visibilityObserver.observe(root);
      }
      updateToggle(); measure();
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

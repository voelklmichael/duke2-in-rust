// The launcher: takes the game files (a zip, a folder or the files), keeps
// them in this browser, lets the player choose the options, then runs the
// engine once per animation frame until the player quits.
// index.html imports this module with the build's version in its address
// (`main.js?v=…`); the same version on the other files keeps the browser from
// mixing in a cached older build.
const VERSION = new URL(import.meta.url).search;
/** The page's title, with the build time index.html put in it. */
const TITLE = document.title;
const { default: init, Game, data_files, tick_hz } = await import(`./pkg/dn2_web.js${VERSION}`);
const { readZip } = await import(`./zip.js${VERSION}`);

const ARCHIVE = 'NUKEM2.CMP';
/** Sound kept queued ahead of the device, in seconds (a game frame is 57 ms). */
const QUEUE_SECONDS = 0.09;
/** Shortest piece of sound worth scheduling, in samples. */
const MIN_CHUNK = 256;
/** Where the launcher's options are kept. */
const OPTIONS_KEY = 'dn2_rust/launcher';

const $ = (id) => document.getElementById(id);

function status(text, isError = false) {
  $('status').hidden = false;
  $('status').textContent = text;
  $('status').className = isError ? 'panel error' : 'panel';
}

// ---- This browser's store: 'files' holds the game files, 'hd' an HD
// pack's pictures, keyed by their path without .png (CZONE1.MNI/solid).

function openDb() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open('dn2_rust', 2);
    request.onupgradeneeded = () => {
      for (const name of ['files', 'hd']) {
        if (!request.result.objectStoreNames.contains(name)) request.result.createObjectStore(name);
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

function done(transaction) {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = resolve;
    transaction.onerror = transaction.onabort = () => reject(transaction.error);
  });
}

/** Everything in object store `name`, or null. */
async function stored(name) {
  try {
    const db = await openDb();
    const store = db.transaction(name).objectStore(name);
    const [keys, values] = await Promise.all(
      [store.getAllKeys(), store.getAll()].map((r) => new Promise((resolve, reject) => {
        r.onsuccess = () => resolve(r.result);
        r.onerror = () => reject(r.error);
      })));
    return keys.length ? new Map(keys.map((k, i) => [k, values[i]])) : null;
  } catch (e) {
    console.warn(`nothing stored in ${name}:`, e);
    return null;
  }
}

/** Replaces what object store `name` holds with `files` (none: empties it). */
async function storeFiles(name, files) {
  try {
    const db = await openDb();
    const transaction = db.transaction(name, 'readwrite');
    const store = transaction.objectStore(name);
    store.clear();
    for (const [key, bytes] of files ?? []) store.put(bytes, key);
    await done(transaction);
  } catch (e) {
    console.warn(`cannot store the files in ${name}:`, e);
  }
}

// ---- Files served next to the page, for development: data/ with the game
// files, hd/ with an HD pack and its list files.txt.

async function fetchFiles() {
  const files = new Map();
  for (const name of data_files()) {
    try {
      const response = await fetch(`data/${name}`);
      if (response.ok) files.set(name, new Uint8Array(await response.arrayBuffer()));
    } catch { /* not there */ }
    // The archive comes first; without it the rest is not needed.
    if (!files.has(ARCHIVE)) return null;
  }
  return files;
}

/** Pictures fetched this many at a time. */
const FETCHES = 16;

async function fetchHd() {
  let names;
  try {
    const response = await fetch('hd/files.txt');
    if (!response.ok) return null;
    names = (await response.text()).split('\n').map((l) => l.trim().replace(/^\.\//, ''))
      .filter((l) => l.toLowerCase().endsWith('.png'));
  } catch {
    return null;
  }
  const pictures = new Map();
  for (let i = 0; i < names.length; i += FETCHES) {
    status(`Loading the HD pack… ${i} of ${names.length}`);
    await Promise.all(names.slice(i, i + FETCHES).map(async (path) => {
      try {
        const response = await fetch(`hd/${path}`);
        if (response.ok) pictures.set(path.slice(0, -4), new Uint8Array(await response.arrayBuffer()));
      } catch { /* left out */ }
    }));
  }
  return pictures.size ? pictures : null;
}

// ---- What the player drops or chooses: files, folders and zips of them,
// sorted into game files and HD pictures.

/** A dropped or chosen file as `{ path, read }`, read() giving its bytes. */
function fromFile(file, path = file.webkitRelativePath || file.name) {
  return { path, read: async () => new Uint8Array(await file.arrayBuffer()) };
}

/** The files of a drop, with those in dropped folders. */
async function droppedFiles(dataTransfer) {
  // The entries must be taken before the first await: the list empties then.
  const entries = [...dataTransfer.items].map((item) => item.webkitGetAsEntry?.()).filter(Boolean);
  if (!entries.length) return [...dataTransfer.files].map((f) => fromFile(f));
  const out = [];
  const walk = async (entry) => {
    if (entry.isFile) {
      const file = await new Promise((resolve, reject) => entry.file(resolve, reject));
      out.push(fromFile(file, entry.fullPath.replace(/^\//, '')));
    } else if (entry.isDirectory) {
      const reader = entry.createReader();
      // readEntries gives a batch at a time, until an empty one.
      for (;;) {
        const batch = await new Promise((resolve, reject) => reader.readEntries(resolve, reject));
        if (!batch.length) break;
        for (const child of batch) await walk(child);
      }
    }
  };
  for (const entry of entries) await walk(entry);
  return out;
}

/** An HD picture's key (CZONE1.MNI/solid) from its path, if it is one. */
function pictureKey(path) {
  if (!/\.png$/i.test(path)) return null;
  const parts = path.split('/');
  // From the part naming the game file: CZONE1.MNI/solid.png, DROP1.MNI.png.
  const at = parts.findIndex((p) => /\.MNI(\.png)?$/i.test(p));
  return at < 0 ? null : parts.slice(at).join('/').slice(0, -4);
}

/**
 * Files that may hold others: zips, and the self-extracting programs of
 * installers, e.g. the shareware's DN2.1.
 */
const ARCHIVES = /\.(zip|exe|\d)$/i;

/** Sorts `items` (archives opened) into `{ game, pictures }` Maps of bytes. */
async function sortFiles(items, progress) {
  const wanted = new Set(data_files());
  const game = new Map();
  const pictures = new Map();
  const take = async (item) => {
    const name = item.path.split('/').pop().toUpperCase();
    if (wanted.has(name)) {
      game.set(name, await item.read());
      return;
    }
    const key = pictureKey(item.path);
    if (key) {
      pictures.set(key, await item.read());
      return;
    }
    if (!ARCHIVES.test(name)) return;
    progress(`Unpacking ${name}…`);
    try {
      const entries = readZip((await item.read()).buffer);
      for (const [i, entry] of entries.entries()) {
        if (i % 200 === 0 && entries.length > 400) progress(`Unpacking ${name}… ${i} of ${entries.length}`);
        await take(entry);
      }
    } catch (e) {
      // Programs are mostly not self-extracting zips, and of no use then.
      if (/\.zip$/i.test(name)) throw new Error(`${name}: ${e.message}`);
      console.info(`${item.path}: ${e.message}`);
    }
  };
  for (const item of items) await take(item);
  return { game, pictures };
}

// ---- The options: kept in local storage, the address may override them.

const DEFAULTS = {
  wide: false, filter: 'nearest', scale: 'auto', hd: true, aspect: 'tall', integer: false,
  smooth: false, fullscreen: false, mute: false, start: 'game', level: 'L1',
};

function loadOptions(params) {
  let options = { ...DEFAULTS };
  try {
    Object.assign(options, JSON.parse(localStorage.getItem(OPTIONS_KEY)) ?? {});
  } catch { /* none kept */ }
  if (params.has('wide')) options.wide = true;
  if (params.has('filter')) {
    const filter = params.get('filter');
    options.filter = ['epx', 'xbr'].includes(filter) ? filter : 'nearest';
  }
  if (params.has('scale')) options.scale = params.get('scale');
  if (params.has('mute')) options.mute = true;
  if (params.has('hd')) options.hd = true;
  if (params.has('square')) options.aspect = 'square';
  if (params.has('integer')) options.integer = true;
  if (params.has('viewer')) options.start = 'viewer';
  for (const start of ['play', 'level']) {
    if (params.has(start)) { options.start = start; options.level = params.get(start) || 'L1'; }
  }
  return options;
}

function saveOptions(options) {
  try {
    localStorage.setItem(OPTIONS_KEY, JSON.stringify(options));
  } catch { /* not kept */ }
}

/** The options for the engine, in the form `Game` reads (a query string). */
function engineOptions(options) {
  const query = [];
  if (options.wide) query.push('wide');
  if (options.filter !== 'nearest') query.push(`filter=${options.filter}`);
  if (options.scale !== 'auto') query.push(`scale=${options.scale}`);
  const level = encodeURIComponent(options.level.trim() || 'L1');
  if (options.start === 'play') query.push(`play=${level}`);
  if (options.start === 'level') query.push(`level=${level}`);
  if (options.start === 'viewer') query.push('viewer');
  return query.join('&');
}

// ---- The launcher.

/**
 * Shows the launcher until the player presses Play or Play mobile; resolves to
 * `{ audio, touch }` for the game, touch for the on-screen controls.
 */
function launch(state) {
  const { options } = state;
  $('status').hidden = true;
  $('launcher').hidden = false;
  document.title = TITLE;

  const fields = [...document.querySelectorAll('#options [name], #options-game [name]')];
  for (const field of fields) {
    if (field.type === 'checkbox') field.checked = options[field.name];
    else field.value = options[field.name];
  }

  const show = () => {
    const game = state.files;
    $('game-state').textContent = game
      ? `ready (${game.size} files${state.fromServer ? ', from the server' : ''})`
      : 'not loaded: drop the game here';
    $('game-state').className = game ? 'ok' : 'missing';
    $('forget-game').style.visibility = !game || state.fromServer ? 'hidden' : '';
    const hd = state.pictures;
    $('hd-state').textContent = hd ? `${hd.size} pictures` : 'none (optional)';
    $('hd-state').className = hd ? 'ok' : 'missing';
    $('forget-hd').style.visibility = hd ? '' : 'hidden';
    $('o-hd').disabled = !hd;
    const needsLevel = options.start === 'play' || options.start === 'level';
    $('o-level').disabled = !needsLevel;
    $('o-level-label').style.opacity = needsLevel ? 1 : 0.5;
    $('play').disabled = $('play-mobile').disabled = !game;
  };
  show();

  for (const field of fields) {
    field.onchange = field.oninput = () => {
      options[field.name] = field.type === 'checkbox' ? field.checked : field.value;
      saveOptions(options);
      show();
    };
  }

  const message = (text, isError = false) => {
    $('drop-message').textContent = text;
    $('drop-message').className = isError ? 'error' : 'ok';
  };
  let busy = false;
  const take = async (getItems) => {
    if (busy) return;
    busy = true;
    $('play-error').textContent = '';
    message('Reading…');
    try {
      const { game, pictures } = await sortFiles(await getItems(), message);
      const found = [];
      if (game.size) {
        if (!game.has(ARCHIVE)) throw new Error(`${ARCHIVE} is not among the files: it holds the game.`);
        state.files = game;
        state.fromServer = false;
        await storeFiles('files', game);
        found.push(`the game (${game.size} files)`);
      }
      if (pictures.size) {
        state.pictures = pictures;
        options.hd = $('o-hd').checked = true;
        saveOptions(options);
        await storeFiles('hd', pictures);
        found.push(`an HD pack (${pictures.size} pictures)`);
      }
      if (!found.length) {
        throw new Error(`Found neither the game (${ARCHIVE}) nor HD pictures (like CZONE1.MNI/solid.png) in that.`);
      }
      message(`Got ${found.join(' and ')}.`);
    } catch (e) {
      console.error(e);
      message(`${e.message ?? e}`, true);
    } finally {
      busy = false;
      show();
    }
  };

  for (const id of ['pick-files', 'pick-dir']) {
    $(id).onchange = (e) => {
      const list = [...e.target.files];
      e.target.value = '';
      take(async () => list.map((f) => fromFile(f)));
    };
  }
  document.body.ondragover = (e) => {
    e.preventDefault();
    if (!$('launcher').hidden) document.body.classList.add('over');
  };
  document.body.ondragleave = (e) => {
    if (!e.relatedTarget) document.body.classList.remove('over');
  };
  document.body.ondrop = (e) => {
    e.preventDefault();
    document.body.classList.remove('over');
    if ($('launcher').hidden) return;
    const items = droppedFiles(e.dataTransfer);
    take(() => items);
  };

  $('forget-game').onclick = async () => {
    state.files = null;
    await storeFiles('files', null);
    message('');
    show();
  };
  $('forget-hd').onclick = async () => {
    state.pictures = null;
    await storeFiles('hd', null);
    message('');
    show();
  };

  return new Promise((resolve) => {
    const start = (touch) => {
      // Both need the click: browsers allow them only on the player's gesture.
      if (options.fullscreen || touch) {
        document.documentElement.requestFullscreen?.()
          .then(() => touch && screen.orientation?.lock?.('landscape'))
          .catch(() => {});
      }
      const audio = !options.mute && window.AudioContext
        ? new AudioContext({ latencyHint: 'interactive' }) : null;
      $('launcher').hidden = true;
      $('play-error').textContent = '';
      resolve({ audio, touch });
    };
    $('play').onclick = () => start(false);
    $('play-mobile').onclick = () => start(true);
  });
}

// ---- Running the game.

/**
 * Shows the on-screen controls and presses `game`'s keys for them, until
 * `signal` aborts.
 */
function touchControls(game, signal) {
  const touch = $('touch');
  touch.hidden = false;
  signal.addEventListener('abort', () => {
    touch.hidden = true;
    for (const held of touch.querySelectorAll('.held')) held.classList.remove('held');
  });
  touch.addEventListener('contextmenu', (e) => e.preventDefault(), { signal });

  /** How many fingers hold each key: one may lift while another still holds it. */
  const holds = new Map();
  const press = (code) => {
    holds.set(code, (holds.get(code) ?? 0) + 1);
    if (holds.get(code) === 1) game.key_down(code);
  };
  const release = (code) => {
    holds.set(code, holds.get(code) - 1);
    if (!holds.get(code)) game.key_up(code);
  };

  // Menu shows a second row with the game's other keys (save, restore, ...);
  // pressing one of them hides it again.
  const menu = $('touch-menu');
  const toggle = $('menu-toggle');
  signal.addEventListener('abort', () => { menu.hidden = true; });
  toggle.addEventListener('pointerdown', (e) => {
    e.preventDefault();
    menu.hidden = !menu.hidden;
    toggle.classList.toggle('held', !menu.hidden);
  }, { signal });
  const closeMenu = () => {
    menu.hidden = true;
    toggle.classList.remove('held');
  };

  for (const button of touch.querySelectorAll('[data-key]')) {
    const code = button.dataset.key;
    const fingers = new Set();
    button.addEventListener('pointerdown', (e) => {
      e.preventDefault();
      button.setPointerCapture(e.pointerId);
      fingers.add(e.pointerId);
      button.classList.add('held');
      press(code);
    }, { signal });
    const up = (e) => {
      if (!fingers.delete(e.pointerId)) return;
      if (!fingers.size) button.classList.remove('held');
      release(code);
      if (menu.contains(button)) closeMenu();
    };
    button.addEventListener('pointerup', up, { signal });
    button.addEventListener('pointercancel', up, { signal });
  }

  // The pad: the direction from its centre to each finger on it, eight ways.
  const pad = $('dpad');
  /** The arrow keys each finger on the pad holds. */
  const fingers = new Map();
  const TAN = Math.tan(Math.PI / 8);
  const aim = (e) => {
    const box = pad.getBoundingClientRect();
    const dx = e.clientX - (box.left + box.width / 2);
    const dy = e.clientY - (box.top + box.height / 2);
    const now = new Set();
    // A small dead zone in the middle.
    if (Math.hypot(dx, dy) > box.width * 0.12) {
      if (Math.abs(dx) > TAN * Math.abs(dy)) now.add(dx < 0 ? 'ArrowLeft' : 'ArrowRight');
      if (Math.abs(dy) > TAN * Math.abs(dx)) now.add(dy < 0 ? 'ArrowUp' : 'ArrowDown');
    }
    const before = fingers.get(e.pointerId) ?? new Set();
    for (const code of before) if (!now.has(code)) release(code);
    for (const code of now) if (!before.has(code)) press(code);
    fingers.set(e.pointerId, now);
  };
  const lift = (e) => {
    for (const code of fingers.get(e.pointerId) ?? []) release(code);
    fingers.delete(e.pointerId);
    pad.classList.toggle('held', fingers.size > 0);
  };
  pad.addEventListener('pointerdown', (e) => {
    e.preventDefault();
    pad.setPointerCapture(e.pointerId);
    pad.classList.add('held');
    aim(e);
  }, { signal });
  pad.addEventListener('pointermove', (e) => { if (fingers.has(e.pointerId)) aim(e); }, { signal });
  pad.addEventListener('pointerup', lift, { signal });
  pad.addEventListener('pointercancel', lift, { signal });
}

/** Runs `game` until the player quits; `touch` shows the on-screen controls. */
function run(game, options, audio, touch) {
  const TICK_HZ = tick_hz();
  /** The rate the music runs at without sound: it still times the movies. */
  const SILENT_RATE = 50 * TICK_HZ;
  const canvas = $('screen');
  const context = canvas.getContext('2d');
  canvas.hidden = false;
  canvas.style.imageRendering = options.smooth ? 'auto' : 'pixelated';
  canvas.focus();
  document.body.classList.add('playing');
  $('status').hidden = true;
  /** Ends the listeners when the game ends. */
  const listening = new AbortController();
  const { signal } = listening;

  // Sound: running after the first key or click if Play did not start it
  // (browsers start it only then). Until it runs, the music is mixed into nothing.
  const resume = () => { if (audio?.state === 'suspended') audio.resume(); };
  /** When the next scheduled piece of sound starts, in audio time. */
  let next = 0;
  /** Samples owed to the silent mix, fractional. */
  let silentOwed = 0;

  function playSound() {
    const rate = audio.sampleRate;
    const now = audio.currentTime;
    if (next < now) next = now + 0.02;
    const length = Math.floor((now + QUEUE_SECONDS - next) * rate);
    if (length < MIN_CHUNK) return;
    const samples = new Float32Array(length);
    game.mix(samples, rate);
    const buffer = audio.createBuffer(1, length, rate);
    buffer.copyToChannel(samples, 0);
    const source = audio.createBufferSource();
    source.buffer = buffer;
    source.connect(audio.destination);
    source.start(next);
    next += length / rate;
  }

  function mixSilently(ticks) {
    const rate = audio ? audio.sampleRate : SILENT_RATE;
    silentOwed += ticks * rate / TICK_HZ;
    const length = Math.floor(silentOwed);
    silentOwed -= length;
    if (length > 0) game.mix(new Float32Array(length), rate);
  }

  // Input.
  addEventListener('keydown', (e) => {
    resume();
    // Repeats change nothing in the engine, but are kept from the browser too.
    if (game.key_down(e.code)) e.preventDefault();
  }, { signal });
  addEventListener('keyup', (e) => { if (game.key_up(e.code)) e.preventDefault(); }, { signal });
  addEventListener('pointerdown', () => { resume(); canvas.focus(); }, { signal });
  addEventListener('blur', () => game.release_all(), { signal });
  document.addEventListener('visibilitychange', () => game.release_all(), { signal });
  if (touch) touchControls(game, signal);

  // The picture: as large as fits, centred, with the original's tall pixels
  // (320x200 fills 4:3) or square ones; with integer scaling, in whole
  // device pixels per canvas pixel.
  const tall = options.aspect === 'tall' ? 6 / 5 : 1;
  let shown = '';
  function fit(width, height) {
    const ratio = devicePixelRatio || 1;
    const key = `${width}x${height}x${innerWidth}x${innerHeight}x${ratio}`;
    if (key === shown) return;
    shown = key;
    canvas.width = width;
    canvas.height = height;
    const roomW = innerWidth * ratio, roomH = innerHeight * ratio;
    let w, h;
    const whole = Math.floor(Math.min(roomW / width, roomH / (height * tall)));
    if (options.integer && whole >= 1) {
      let tallWhole = Math.round(whole * tall);
      if (height * tallWhole > roomH) tallWhole--;
      w = width * whole;
      h = height * Math.max(1, tallWhole);
    } else {
      const aspect = width / (height * tall);
      w = roomW; h = roomW / aspect;
      if (h > roomH) { h = roomH; w = h * aspect; }
    }
    const left = Math.round((roomW - w) / 2);
    // With the on-screen controls, at the top: in portrait they go below it.
    const top = touch ? 0 : Math.round((roomH - h) / 2);
    Object.assign(canvas.style, {
      width: `${w / ratio}px`, height: `${h / ratio}px`,
      left: `${left / ratio}px`, top: `${top / ratio}px`,
    });
  }

  return new Promise((resolve) => {
    let last = performance.now();
    let ticksOwed = 0;
    function frame(time) {
      // The first frame's time may be earlier than the start.
      ticksOwed += Math.max(0, time - last) / 1000 * TICK_HZ;
      last = Math.max(last, time);
      let ticks = Math.floor(ticksOwed);
      // After a stall (a hidden tab) the rest is dropped, not caught up on.
      if (ticks > TICK_HZ) { ticks = TICK_HZ; ticksOwed = 0; } else ticksOwed -= ticks;
      if (!game.update(ticks)) {
        listening.abort();
        audio?.close();
        game.free();
        canvas.hidden = true;
        document.body.classList.remove('playing');
        if (document.fullscreenElement) document.exitFullscreen().catch(() => {});
        resolve();
        return;
      }
      if (audio?.state === 'running') playSound(); else mixSilently(ticks);
      const image = game.render();
      fit(image.width, image.height);
      context.putImageData(image, 0, 0);
      requestAnimationFrame(frame);
    }
    requestAnimationFrame(frame);
  });
}

async function main() {
  await init({ module_or_path: new URL(`pkg/dn2_web_bg.wasm${VERSION}`, import.meta.url) });
  const params = new URLSearchParams(location.search);
  const state = { options: loadOptions(params), files: null, pictures: null, fromServer: false };
  status('Looking for the game files…');
  state.files = await fetchFiles();
  state.fromServer = !!state.files;
  if (!state.files && !params.has('pick')) {
    const files = await stored('files');
    state.files = files?.has(ARCHIVE) ? files : null;
  }
  state.pictures = await fetchHd() ?? await stored('hd');

  let start = params.has('go') && state.files;
  for (;;) {
    let audio = null;
    let touch = params.has('touch');
    if (start) {
      // Sound starts suspended here, until the first key or click.
      if (!state.options.mute && window.AudioContext) audio = new AudioContext({ latencyHint: 'interactive' });
    } else {
      ({ audio, touch } = await launch(state));
    }
    start = false;
    status('Loading the game…');
    // Let the status show before the engine takes the thread.
    await new Promise((resolve) => requestAnimationFrame(() => setTimeout(resolve)));
    const pictures = state.options.hd ? state.pictures : null;
    let game;
    try {
      game = new Game(state.files, pictures, engineOptions(state.options));
    } catch (e) {
      console.error(e);
      audio?.close();
      $('play-error').textContent = `${e.message ?? e}`;
      continue;
    }
    await run(game, state.options, audio, touch);
  }
}

main().catch((e) => {
  console.error(e);
  status(`${e.message ?? e}`, true);
});

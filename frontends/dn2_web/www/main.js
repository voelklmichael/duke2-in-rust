// Gets the game files, then runs the engine once per animation frame.
import init, { Game, data_files, tick_hz } from './pkg/dn2_web.js';

const ARCHIVE = 'NUKEM2.CMP';
/** Sound kept queued ahead of the device, in seconds (a game frame is 57 ms). */
const QUEUE_SECONDS = 0.09;
/** Shortest piece of sound worth scheduling, in samples. */
const MIN_CHUNK = 256;

const $ = (id) => document.getElementById(id);

function status(text, isError = false) {
  $('status').hidden = false;
  $('status').textContent = text;
  $('status').className = isError ? 'panel error' : 'panel';
}

// ---- Game files: the server's data/ folder, else this browser's store, else ask.

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

function openDb() {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open('dn2_rust', 2);
    request.onupgradeneeded = () => {
      // 'files': the game files; 'hd': an HD pack's pictures.
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

async function storedFiles() {
  const files = await stored('files');
  return files?.has(ARCHIVE) ? files : null;
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

/** Replaces what object store `name` holds with `files`. */
async function storeFiles(files, name = 'files') {
  try {
    const db = await openDb();
    const transaction = db.transaction(name, 'readwrite');
    const store = transaction.objectStore(name);
    store.clear();
    for (const [name, bytes] of files) store.put(bytes, name);
    await done(transaction);
  } catch (e) {
    console.warn(`cannot store the files in ${name}:`, e);
  }
}

// ---- An HD pack (?hd): the server's hd/ folder with its list files.txt,
// else the one stored in this browser, else ask (?pickhd asks again).
// Pictures are keyed by their path without .png, e.g. CZONE1.MNI/solid.

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

/** Shows the folder picker for an HD pack; null if the player skips it. */
function pickHd() {
  $('status').hidden = true;
  $('pick-hd').hidden = false;
  return new Promise((resolve) => {
    const finish = (pictures) => { $('pick-hd').hidden = true; resolve(pictures); };
    $('pick-hd-dir').onchange = async (e) => {
      const pictures = new Map();
      for (const file of e.target.files) {
        // The path below the chosen folder.
        const path = file.webkitRelativePath.split('/').slice(1).join('/');
        if (path.toLowerCase().endsWith('.png')) {
          pictures.set(path.slice(0, -4), new Uint8Array(await file.arrayBuffer()));
        }
      }
      if (!pictures.size) {
        $('pick-hd-error').textContent = 'There are no PNG pictures in that folder.';
        return;
      }
      finish(pictures);
    };
    $('pick-hd-skip').onclick = () => finish(null);
  });
}

async function hdPack(params) {
  if (!params.has('hd') && !params.has('pickhd')) return null;
  if (!params.has('pickhd')) {
    const found = await fetchHd() ?? await stored('hd');
    if (found) return found;
  }
  const pictures = await pickHd();
  if (pictures) await storeFiles(pictures, 'hd');
  return pictures;
}

/** Shows the file picker until the chosen files include the archive. */
function pickFiles() {
  $('status').hidden = true;
  $('pick').hidden = false;
  const wanted = new Set(data_files());
  return new Promise((resolve) => {
    const take = async (list) => {
      const files = new Map();
      for (const file of list) {
        const name = file.name.toUpperCase();
        if (wanted.has(name)) files.set(name, new Uint8Array(await file.arrayBuffer()));
      }
      if (!files.has(ARCHIVE)) {
        $('pick-error').textContent = `${ARCHIVE} is not among the chosen files.`;
        return;
      }
      $('pick').hidden = true;
      resolve(files);
    };
    for (const id of ['pick-dir', 'pick-files']) {
      $(id).onchange = (e) => take(e.target.files);
    }
    const drop = $('drop');
    drop.ondragover = (e) => { e.preventDefault(); drop.classList.add('over'); };
    drop.ondragleave = () => drop.classList.remove('over');
    drop.ondrop = (e) => {
      e.preventDefault();
      drop.classList.remove('over');
      take(e.dataTransfer.files);
    };
  });
}

// ---- Running the game.

function run(game, params) {
  const TICK_HZ = tick_hz();
  /** The rate the music runs at without sound: it still times the movies. */
  const SILENT_RATE = 50 * TICK_HZ;
  const canvas = $('screen');
  const context = canvas.getContext('2d');
  canvas.hidden = false;
  canvas.focus();
  $('status').hidden = true;

  // Sound: created now, running after the first key or click (browsers
  // start it only then). Until it runs, the music is mixed into nothing.
  let audio = null;
  if (!params.has('mute') && window.AudioContext) {
    audio = new AudioContext({ latencyHint: 'interactive' });
  }
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
  });
  addEventListener('keyup', (e) => { if (game.key_up(e.code)) e.preventDefault(); });
  addEventListener('pointerdown', () => { resume(); canvas.focus(); });
  addEventListener('blur', () => game.release_all());
  document.addEventListener('visibilitychange', () => game.release_all());

  // The picture: as large as fits, with the original's tall pixels
  // (320x200 fills 4:3), centred.
  let shown = '';
  function fit(width, height) {
    const key = `${width}x${height}x${innerWidth}x${innerHeight}`;
    if (key === shown) return;
    shown = key;
    canvas.width = width;
    canvas.height = height;
    const aspect = (width * 5) / (height * 6);
    let w = innerWidth, h = innerWidth / aspect;
    if (h > innerHeight) { h = innerHeight; w = h * aspect; }
    Object.assign(canvas.style, {
      width: `${w}px`, height: `${h}px`,
      left: `${(innerWidth - w) / 2}px`, top: `${(innerHeight - h) / 2}px`,
    });
  }

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
      audio?.close();
      canvas.hidden = true;
      status('The game has ended. Reload the page to play again.');
      return;
    }
    if (audio?.state === 'running') playSound(); else mixSilently(ticks);
    const image = game.render();
    fit(image.width, image.height);
    context.putImageData(image, 0, 0);
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}

async function main() {
  await init();
  const params = new URLSearchParams(location.search);
  let files = await fetchFiles();
  if (!files && !params.has('pick')) files = await storedFiles();
  let pictures;
  while (true) {
    if (!files) {
      files = await pickFiles();
      await storeFiles(files);
    }
    if (pictures === undefined) pictures = await hdPack(params);
    status('Loading the game…');
    // Let the status show before the engine takes the thread.
    await new Promise((resolve) => requestAnimationFrame(() => setTimeout(resolve)));
    try {
      return run(new Game(files, pictures, location.search), params);
    } catch (e) {
      console.error(e);
      files = null;
      $('pick-error').textContent = `${e.message ?? e}`;
    }
  }
}

main().catch((e) => {
  console.error(e);
  status(`${e.message ?? e}`, true);
});

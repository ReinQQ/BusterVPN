import { mkdir, writeFile, stat } from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const execFileAsync = promisify(execFile);

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUTPUT_ROOT = resolve(__dirname, '..', 'public', 'ponies');
const RAW_BASE = 'https://raw.githubusercontent.com/RoosterDragon/Desktop-Ponies/master/Content/Ponies';

// All source sprites face right so scaleX(-1) mirrors cleanly for leftward motion.
const PONY_MANIFEST = [
  {
    id: 'twilight-sparkle',
    folder: 'Twilight Sparkle',
    files: {
      stand: 'stand_twilight_right.gif',
      walk: 'trotcycle_twilight_right.gif',
      special: 'magic_twilight_right.gif',
      sleep: 'read.gif',
    },
  },
  {
    id: 'rainbow-dash',
    folder: 'Rainbow Dash',
    files: {
      stand: 'stand_rainbow_right.gif',
      walk: 'trotcycle_rainbow_right.gif',
      special: 'fly_rainbow_right.gif',
      sleep: 'cloud_sleep_right.gif',
    },
  },
  {
    id: 'applejack',
    folder: 'Applejack',
    files: {
      stand: 'stand_aj_right.gif',
      walk: 'trotcycle_aj_right.gif',
      special: 'aj_lasso_right.gif',
      sleep: 'sleep_right.gif',
    },
  },
  {
    id: 'rarity',
    folder: 'Rarity',
    files: {
      stand: 'stand_rarity_right.gif',
      walk: 'trotcycle_rarity_right.gif',
      special: 'horn_right.gif',
      sleep: 'sleep_right.gif',
    },
  },
  {
    id: 'fluttershy',
    folder: 'Fluttershy',
    files: {
      stand: 'stand_fluttershy_right.gif',
      walk: 'trotcycle_fluttershy_right.gif',
      special: 'fly_fluttershy_right.gif',
      sleep: 'sleeping_fluttershy_right.gif',
    },
  },
  {
    id: 'pinkie-pie',
    folder: 'Pinkie Pie',
    files: {
      stand: 'stand_pinkiepie_right.gif',
      walk: 'trotcycle_pinkiepie_right.gif',
      special: 'partycannon_pinkie_pie_right.gif',
      sleep: 'pinkie-sleep-right.gif',
    },
  },
  {
    id: 'starlight-glimmer',
    folder: 'Starlight Glimmer',
    files: {
      stand: 'starlightglimmer-idle-right.gif',
      walk: 'starlightglimmer-trot-right.gif',
      special: 'starlightglimmer-popcorn-right.gif',
      sleep: 'starlightglimmer-idle-right.gif',
    },
  },
  {
    id: 'sunset-shimmer',
    folder: 'Sunset Shimmer',
    files: {
      stand: 'sunsetshimmer-idle-right.gif',
      walk: 'sunsetshimmer-right.gif',
      special: 'sunset-teleport-start-right.gif',
      sleep: 'sunsetshimmer-idle-right.gif',
    },
  },
  {
    id: 'celestia',
    folder: 'Princess Celestia',
    files: {
      stand: 'stand_right.gif',
      walk: 'walk_right.gif',
      special: 'fly_right.gif',
      sleep: 'stand_right.gif',
    },
  },
  {
    id: 'luna',
    folder: 'Princess Luna',
    files: {
      stand: 'luna_idle_right.gif',
      walk: 'luna_walk_right.gif',
      special: 'luna-flight-right.gif',
      sleep: 'luna_idle_right.gif',
    },
  },
  {
    id: 'cadence',
    folder: 'Princess Cadance',
    files: {
      stand: 'stand_right.gif',
      walk: 'cadance-trot-right.gif',
      special: 'cadance-flight-right.gif',
      sleep: 'stand_right.gif',
    },
  },
  {
    id: 'trixie',
    folder: 'Trixie',
    files: {
      stand: 'stand_right.gif',
      walk: 'walking_right.gif',
      special: 'trixie_fireworks_right.gif',
      sleep: 'sit_right.gif',
    },
  },
  {
    id: 'dj-pon3',
    folder: 'Vinyl Scratch',
    files: {
      stand: 'idle_scratch_right.gif',
      walk: 'trotcycle_scratch_right.gif',
      special: 'djdecks_playing_strobe1_right.gif',
      sleep: 'idle_smile_scratch_right.gif',
    },
  },
];

function isValidGif(buffer) {
  if (!buffer || buffer.length < 14) return false;
  const header = buffer.subarray(0, 6).toString('ascii');
  return header === 'GIF89a' || header === 'GIF87a';
}

let preferCurl = false;

async function fetchViaCurl(url) {
  const curlBin = process.platform === 'win32' ? 'curl.exe' : 'curl';
  const { stdout } = await execFileAsync(
    curlBin,
    [
      '-fsSL',
      '--connect-timeout',
      '10',
      '--max-time',
      '30',
      '-A',
      'BusterVPN-PonyFetcher/1.6.0',
      url,
    ],
    { encoding: 'buffer', maxBuffer: 20 * 1024 * 1024 }
  );
  return stdout;
}

async function fetchBufferWithRetry(url, retries = 3) {
  let lastErr;
  for (let attempt = 1; attempt <= retries; attempt++) {
    try {
      let buf;
      if (!preferCurl) {
        try {
          const controller = new AbortController();
          const timer = setTimeout(() => controller.abort(), 5000);
          try {
            const res = await fetch(url, {
              headers: { 'User-Agent': 'BusterVPN-PonyFetcher/1.6.0' },
              signal: controller.signal,
            });
            if (!res.ok) {
              throw new Error(`HTTP ${res.status} for ${url}`);
            }
            buf = Buffer.from(await res.arrayBuffer());
          } finally {
            clearTimeout(timer);
          }
        } catch (fetchErr) {
          preferCurl = true;
          buf = await fetchViaCurl(url);
        }
      } else {
        buf = await fetchViaCurl(url);
      }

      if (!isValidGif(buf)) {
        throw new Error(`Invalid GIF header for ${url} (${buf.length} bytes)`);
      }
      return buf;
    } catch (err) {
      lastErr = err;
      if (attempt < retries) {
        await new Promise((r) => setTimeout(r, 500 * attempt));
      }
    }
  }
  throw lastErr;
}

async function fetchPony(entry) {
  const dir = join(OUTPUT_ROOT, entry.id);
  await mkdir(dir, { recursive: true });

  const cache = new Map();
  let downloaded = 0;

  for (const [action, remoteFile] of Object.entries(entry.files)) {
    let buf = cache.get(remoteFile);
    if (!buf) {
      const url = `${RAW_BASE}/${encodeURIComponent(entry.folder)}/${encodeURIComponent(remoteFile)}`;
      buf = await fetchBufferWithRetry(url);
      cache.set(remoteFile, buf);
    }
    const targetPath = join(dir, `${action}.gif`);
    await writeFile(targetPath, buf);
    const info = await stat(targetPath);
    if (info.size === 0) {
      throw new Error(`Zero-byte file written: ${targetPath}`);
    }
    downloaded += 1;
  }

  return { id: entry.id, count: downloaded };
}

async function main() {
  console.log(`Fetching Desktop Ponies sprites into ${OUTPUT_ROOT} ...`);
  const results = [];
  for (const entry of PONY_MANIFEST) {
    const res = await fetchPony(entry);
    results.push(res);
    console.log(`  [OK] ${res.id}: ${res.count} GIFs verified`);
  }

  const totalFiles = results.reduce((acc, r) => acc + r.count, 0);
  console.log(`Done! Verified ${results.length} ponies (${totalFiles} GIF files).`);
}

main().catch((err) => {
  console.error('Failed to fetch pony sprites:', err);
  process.exit(1);
});

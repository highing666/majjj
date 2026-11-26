import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const webRoot = path.resolve(__dirname, "..");
const personasSrcDir = path.join(webRoot, "assets-src/personas");
const atlasOutDir = path.join(webRoot, "src/assets/atlas");

const PERSONA_SPECS = [
  {
    id: "QuickHothead",
    name: "急眼快攻哥",
    bgTop: "#8B2719",
    bgBottom: "#3D100C",
    accent: "#FF7B47",
    skin: "#F4C29B",
    skinShadow: "#D4956E",
    hair: "#2A1512",
    outfit: "#C93B2B",
    outfitTrim: "#F5C663",
    archetype: "hothead",
  },
  {
    id: "TeaSarcasm",
    name: "阴阳茶艺师",
    bgTop: "#1B5E4E",
    bgBottom: "#0D2B25",
    accent: "#5FE3BA",
    skin: "#F8D7BF",
    skinShadow: "#DBAF93",
    hair: "#1A1E24",
    outfit: "#176B5B",
    outfitTrim: "#E8C36B",
    archetype: "tea",
  },
  {
    id: "StubbornBlamer",
    name: "嘴硬死磕王",
    bgTop: "#24477A",
    bgBottom: "#101E38",
    accent: "#6DA8FF",
    skin: "#EAB892",
    skinShadow: "#C48A64",
    hair: "#1F232C",
    outfit: "#2A3E63",
    outfitTrim: "#E58E3C",
    archetype: "stubborn",
  },
  {
    id: "ChaosInstigator",
    name: "拱火乐子人",
    bgTop: "#7A4716",
    bgBottom: "#2F190A",
    accent: "#FFC145",
    skin: "#F5CA9F",
    skinShadow: "#D39D6F",
    hair: "#3C2118",
    outfit: "#6C3483",
    outfitTrim: "#F7B733",
    archetype: "chaos",
  },
  {
    id: "TiltedMaster",
    name: "红温老雀神",
    bgTop: "#6F1F32",
    bgBottom: "#2B0B14",
    accent: "#F26B7A",
    skin: "#E8B794",
    skinShadow: "#C48D69",
    hair: "#6B7280",
    outfit: "#6E1D2A",
    outfitTrim: "#E6B85C",
    archetype: "master",
  },
];

const MOODS = ["Smug", "Mocking", "Tilted", "Nervous", "Plotting", "Deadpan"];

const GRID = {
  cols: MOODS.length, // 6
  rows: PERSONA_SPECS.length, // 5
  cellW: 256,
  cellH: 256,
  frameW: 248,
  frameH: 248,
  pad: 4,
};

function moodSvgLayer(mood, accent) {
  switch (mood) {
    case "Smug":
      return `
        <path d="M84 114 Q100 104 114 113" fill="none" stroke="#181A20" stroke-width="5" stroke-linecap="round"/>
        <path d="M134 109 Q150 96 168 106" fill="none" stroke="#181A20" stroke-width="5" stroke-linecap="round"/>
        <ellipse cx="99" cy="128" rx="9" ry="7" fill="#181A20"/>
        <ellipse cx="149" cy="126" rx="9" ry="7" fill="#181A20"/>
        <circle cx="96" cy="125" r="2.6" fill="#FFFDF8"/>
        <circle cx="146" cy="123" r="2.6" fill="#FFFDF8"/>
        <path d="M106 164 Q129 175 150 156" fill="none" stroke="#181A20" stroke-width="4.5" stroke-linecap="round"/>
        <path d="M131 164 L144 160 L141 167 Z" fill="#FFFDF8"/>
        <path d="M200 44 L206 58 L220 64 L206 70 L200 84 L194 70 L180 64 L194 58 Z" fill="#FFE074" stroke="#181A20" stroke-width="2.5"/>`;
    case "Mocking":
      return `
        <path d="M84 112 Q100 107 115 114" fill="none" stroke="#181A20" stroke-width="4.8" stroke-linecap="round"/>
        <path d="M134 113 Q150 101 168 109" fill="none" stroke="#181A20" stroke-width="4.8" stroke-linecap="round"/>
        <path d="M86 127 L112 127" stroke="#181A20" stroke-width="4.2" stroke-linecap="round"/>
        <path d="M136 125 L162 125" stroke="#181A20" stroke-width="4.2" stroke-linecap="round"/>
        <circle cx="104" cy="131" r="5" fill="#181A20"/>
        <circle cx="154" cy="129" r="5" fill="#181A20"/>
        <path d="M108 164 Q124 163 145 156" fill="none" stroke="#181A20" stroke-width="4.2" stroke-linecap="round"/>
        <path d="M192 52 Q204 46 212 58 Q202 64 192 52 Z" fill="${accent}" stroke="#181A20" stroke-width="2.4"/>`;
    case "Tilted":
      return `
        <ellipse cx="124" cy="136" rx="54" ry="44" fill="#E6392E" opacity="0.28"/>
        <path d="M84 106 L115 119" stroke="#181A20" stroke-width="6" stroke-linecap="round"/>
        <path d="M166 106 L135 119" stroke="#181A20" stroke-width="6" stroke-linecap="round"/>
        <circle cx="99" cy="129" r="8" fill="#FFFDF8" stroke="#181A20" stroke-width="3.2"/>
        <circle cx="149" cy="129" r="8" fill="#FFFDF8" stroke="#181A20" stroke-width="3.2"/>
        <circle cx="100" cy="129" r="3.5" fill="#181A20"/>
        <circle cx="148" cy="129" r="3.5" fill="#181A20"/>
        <path d="M101 168 Q124 150 147 168 Q124 180 101 168 Z" fill="#7D1518" stroke="#181A20" stroke-width="3.5"/>
        <path d="M188 46 L198 56 M206 46 L196 56 M192 64 L202 58 M192 38 L202 44" stroke="#FF4A3D" stroke-width="4.5" stroke-linecap="round"/>`;
    case "Nervous":
      return `
        <path d="M84 116 Q98 106 114 110" fill="none" stroke="#181A20" stroke-width="4.5" stroke-linecap="round"/>
        <path d="M134 110 Q150 106 164 116" fill="none" stroke="#181A20" stroke-width="4.5" stroke-linecap="round"/>
        <circle cx="99" cy="128" r="10" fill="#FFFDF8" stroke="#181A20" stroke-width="3.2"/>
        <circle cx="149" cy="128" r="10" fill="#FFFDF8" stroke="#181A20" stroke-width="3.2"/>
        <circle cx="101" cy="129" r="3" fill="#181A20"/>
        <circle cx="151" cy="129" r="3" fill="#181A20"/>
        <path d="M104 164 Q112 159 120 165 Q128 170 136 163 Q142 159 147 164" fill="none" stroke="#181A20" stroke-width="4" stroke-linecap="round"/>
        <path d="M196 50 Q212 72 196 86 Q180 72 196 50 Z" fill="#6EC6FF" stroke="#181A20" stroke-width="3"/>`;
    case "Plotting":
      return `
        <rect x="68" y="88" width="112" height="44" rx="18" fill="#111622" opacity="0.45"/>
        <path d="M84 112 L115 118" stroke="#181A20" stroke-width="5.2" stroke-linecap="round"/>
        <path d="M164 112 L133 118" stroke="#181A20" stroke-width="5.2" stroke-linecap="round"/>
        <polygon points="88,128 112,124 108,134 90,134" fill="#FFE074" stroke="#181A20" stroke-width="2.6"/>
        <polygon points="160,128 136,124 140,134 158,134" fill="#FFE074" stroke="#181A20" stroke-width="2.6"/>
        <path d="M102 161 Q124 178 148 159" fill="none" stroke="#181A20" stroke-width="4.5" stroke-linecap="round"/>`;
    default: // Deadpan
      return `
        <line x1="85" y1="114" x2="113" y2="114" stroke="#181A20" stroke-width="4.6" stroke-linecap="round"/>
        <line x1="135" y1="114" x2="163" y2="114" stroke="#181A20" stroke-width="4.6" stroke-linecap="round"/>
        <line x1="87" y1="129" x2="111" y2="129" stroke="#181A20" stroke-width="4.4" stroke-linecap="round"/>
        <line x1="137" y1="129" x2="161" y2="129" stroke="#181A20" stroke-width="4.4" stroke-linecap="round"/>
        <line x1="110" y1="164" x2="138" y2="164" stroke="#181A20" stroke-width="4.2" stroke-linecap="round"/>
        <line x1="186" y1="46" x2="186" y2="82" stroke="#89A4B8" stroke-width="4" stroke-linecap="round"/>
        <line x1="198" y1="42" x2="198" y2="86" stroke="#89A4B8" stroke-width="4" stroke-linecap="round"/>
        <line x1="210" y1="48" x2="210" y2="80" stroke="#89A4B8" stroke-width="4" stroke-linecap="round"/>`;
  }
}

function buildPersonaSvg(persona, mood) {
  return `<svg viewBox="0 0 248 248" xmlns="http://www.w3.org/2000/svg">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0%" stop-color="${persona.bgTop}"/>
      <stop offset="100%" stop-color="${persona.bgBottom}"/>
    </linearGradient>
  </defs>
  <rect x="4" y="4" width="240" height="240" rx="44" fill="url(#bg)" stroke="#181A20" stroke-width="6"/>
  <path d="M26 236 Q52 178 124 176 Q196 178 222 236 Z" fill="${persona.outfit}" stroke="#181A20" stroke-width="5"/>
  <path d="M98 182 L124 210 L150 182" fill="none" stroke="${persona.outfitTrim}" stroke-width="6" stroke-linecap="round" stroke-linejoin="round"/>
  <rect x="108" y="160" width="32" height="28" rx="10" fill="${persona.skinShadow}" stroke="#181A20" stroke-width="4"/>
  <ellipse cx="124" cy="126" rx="56" ry="62" fill="${persona.skin}" stroke="#181A20" stroke-width="5"/>
  <path d="M68 118 Q68 58 124 54 Q180 58 180 118 Q162 88 124 88 Q86 88 68 118 Z" fill="${persona.hair}" stroke="#181A20" stroke-width="5"/>
  ${moodSvgLayer(mood, persona.accent)}
  <rect x="8" y="8" width="232" height="232" rx="40" fill="none" stroke="${persona.accent}" stroke-width="3.5" opacity="0.72"/>
</svg>\n`;
}

function writePersonaSvgs() {
  fs.mkdirSync(personasSrcDir, { recursive: true });
  for (const persona of PERSONA_SPECS) {
    for (const mood of MOODS) {
      const filename = `persona_${persona.id}_${mood}.svg`;
      fs.writeFileSync(
        path.join(personasSrcDir, filename),
        buildPersonaSvg(persona, mood),
        "utf8",
      );
    }
  }
}

function writeSpritesheetJson() {
  const atlasW = GRID.cols * GRID.cellW; // 1536 <= 2048
  const atlasH = GRID.rows * GRID.cellH; // 1280 <= 2048
  const frames = {};

  PERSONA_SPECS.forEach((persona, row) => {
    MOODS.forEach((mood, col) => {
      const key = `persona_${persona.id}_${mood}`;
      frames[key] = {
        frame: {
          x: col * GRID.cellW + GRID.pad,
          y: row * GRID.cellH + GRID.pad,
          w: GRID.frameW,
          h: GRID.frameH,
        },
        rotated: false,
        trimmed: false,
        spriteSourceSize: { x: 0, y: 0, w: GRID.frameW, h: GRID.frameH },
        sourceSize: { w: GRID.frameW, h: GRID.frameH },
      };
    });
  });

  const sheetJson = {
    frames,
    meta: {
      app: "majjj-persona-atlas-builder",
      version: "1.0",
      image: "personas@2x.png",
      format: "RGBA8888",
      size: { w: atlasW, h: atlasH },
      scale: "2",
    },
  };

  fs.mkdirSync(atlasOutDir, { recursive: true });
  fs.writeFileSync(
    path.join(atlasOutDir, "personas@2x.json"),
    `${JSON.stringify(sheetJson, null, 2)}\n`,
    "utf8",
  );
}

function rasterizePersonasAtlas() {
  const swiftScript = path.join(__dirname, "rasterize-personas.swift");
  const swiftBin = fs.existsSync("/Library/Developer/CommandLineTools/usr/bin/swift")
    ? "/Library/Developer/CommandLineTools/usr/bin/swift"
    : "swift";
  const env = { ...process.env };
  if (
    process.platform === "darwin" &&
    !env.DEVELOPER_DIR &&
    fs.existsSync("/Library/Developer/CommandLineTools")
  ) {
    env.DEVELOPER_DIR = "/Library/Developer/CommandLineTools";
  }

  const cacheDir = path.join(webRoot, ".swift-module-cache-personas");
  fs.mkdirSync(cacheDir, { recursive: true });
  env.CLANG_MODULE_CACHE_PATH = cacheDir;
  env.SWIFT_MODULE_CACHE_PATH = cacheDir;

  try {
    execFileSync(
      swiftBin,
      [
        "-module-cache-path",
        cacheDir,
        swiftScript,
        path.join(atlasOutDir, "personas@2x.png"),
      ],
      { env, stdio: "inherit" },
    );
  } finally {
    fs.rmSync(cacheDir, { recursive: true, force: true });
  }
}

writePersonaSvgs();
writeSpritesheetJson();
rasterizePersonasAtlas();
console.log("Built @2x comic impasto persona atlas (1536x1280, 5 personas x 6 moods = 30 frames).");

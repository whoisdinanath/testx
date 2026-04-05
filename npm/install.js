"use strict";

const { execSync } = require("child_process");
const fs = require("fs");
const https = require("https");
const os = require("os");
const path = require("path");
const zlib = require("zlib");

const VERSION = require("./package.json").version;
const REPO = "whoisdinanath/testx";

function getPlatform() {
  const platform = os.platform();
  const arch = os.arch();

  const targets = {
    "darwin-x64": { name: "macos-x86_64", archive: "tar.gz" },
    "darwin-arm64": { name: "macos-aarch64", archive: "tar.gz" },
    "linux-x64": { name: "linux-x86_64", archive: "tar.gz" },
    "linux-arm64": { name: "linux-aarch64", archive: "tar.gz" },
    "win32-x64": { name: "windows-x86_64", archive: "zip" },
  };

  const key = `${platform}-${arch}`;
  const target = targets[key];
  if (!target) {
    console.error(`Unsupported platform: ${key}`);
    console.error("Install from source: cargo install testx-cli");
    process.exit(1);
  }
  return target;
}

function download(url) {
  return new Promise((resolve, reject) => {
    https
      .get(url, (res) => {
        if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
          return download(res.headers.location).then(resolve, reject);
        }
        if (res.statusCode !== 200) {
          return reject(new Error(`HTTP ${res.statusCode} from ${url}`));
        }
        const chunks = [];
        res.on("data", (chunk) => chunks.push(chunk));
        res.on("end", () => resolve(Buffer.concat(chunks)));
        res.on("error", reject);
      })
      .on("error", reject);
  });
}

function extractTarGz(buffer, destDir) {
  const tmpFile = path.join(os.tmpdir(), `testx-${Date.now()}.tar.gz`);
  fs.writeFileSync(tmpFile, buffer);
  try {
    execSync(`tar xzf "${tmpFile}" -C "${destDir}"`, { stdio: "pipe" });
  } finally {
    fs.unlinkSync(tmpFile);
  }
}

function extractZip(buffer, destDir) {
  const tmpFile = path.join(os.tmpdir(), `testx-${Date.now()}.zip`);
  fs.writeFileSync(tmpFile, buffer);
  try {
    if (os.platform() === "win32") {
      execSync(
        `powershell -Command "Expand-Archive -Path '${tmpFile}' -DestinationPath '${destDir}' -Force"`,
        { stdio: "pipe" }
      );
    } else {
      execSync(`unzip -o "${tmpFile}" -d "${destDir}"`, { stdio: "pipe" });
    }
  } finally {
    fs.unlinkSync(tmpFile);
  }
}

async function main() {
  const target = getPlatform();
  const binDir = path.join(__dirname, "bin");
  const binName = os.platform() === "win32" ? "testx.exe" : "testx";
  const binPath = path.join(binDir, binName);

  // Skip download if binary already exists (e.g., from CI pre-packaging)
  if (fs.existsSync(binPath)) {
    console.log("testx binary already present, skipping download.");
    return;
  }

  const url = `https://github.com/${REPO}/releases/download/v${VERSION}/testx-v${VERSION}-${target.name}.${target.archive}`;
  console.log(`Downloading testx v${VERSION} for ${target.name}...`);

  try {
    const buffer = await download(url);

    fs.mkdirSync(binDir, { recursive: true });

    if (target.archive === "tar.gz") {
      extractTarGz(buffer, binDir);
    } else {
      extractZip(buffer, binDir);
    }

    // Ensure executable permission on Unix
    if (os.platform() !== "win32") {
      fs.chmodSync(binPath, 0o755);
    }

    console.log(`testx v${VERSION} installed successfully.`);
  } catch (err) {
    console.error(`Failed to install testx: ${err.message}`);
    console.error("Install from source: cargo install testx-cli");
    process.exit(1);
  }
}

main();

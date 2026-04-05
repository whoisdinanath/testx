# npm distribution

This directory contains the npm wrapper for distributing `testx` as a Node.js package.

## How it works

1. `npm install @whoisdinanath/testx` runs the `postinstall` hook
2. `install.js` detects the platform and downloads the prebuilt binary from GitHub Releases
3. `bin/testx` is a thin Node.js wrapper that forwards all arguments to the native binary

## Supported platforms

| Platform | Architecture | Binary |
| -------- | ------------ | ------ |
| macOS    | x64          | ✅     |
| macOS    | ARM64        | ✅     |
| Linux    | x64          | ✅     |
| Linux    | ARM64        | ✅     |
| Windows  | x64          | ✅     |

## Publishing

```bash
cd npm
npm publish
```

The version in `package.json` must match the GitHub release tag.

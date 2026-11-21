# Cutting a Release

A release is a version tag on `main`. Pushing the tag starts the
`canto-releases` workflow (`.github/workflows/canto-releases.yml`), which builds
the Flatpak bundles for x86_64 and aarch64 and publishes a GitHub release with
them. Pushes to `main` without a tag only build and check; they publish nothing.

## 1. Bump the version

The version lives in four places. Change all of them in one commit:

- `meson.build`: `version:` in `project()`.
- `Cargo.toml`: `version` of the `canto` package.
- `Cargo.lock`: the `canto` entry. Let cargo update it (for example with
  `cargo update -p canto --offline`) rather than editing it by hand.
- `data/io.github.gusthanks.Canto.metainfo.xml`: a new `<release>` at the top
  of `<releases>`, with the version, the date and a short description of
  what changed.

## 2. Validate the metainfo

```sh
appstreamcli validate --no-net --pedantic data/io.github.gusthanks.Canto.metainfo.xml
```

`meson test` runs the same check when `appstreamcli` is installed.

## 3. Dependencies

If any dependency changed since the last release, regenerate the Flatpak cargo
sources and commit them, or the offline Flatpak build fails:

```sh
./scripts/build.sh cargo-src   # writes flatpak/cargo-sources.json
```

## 4. Check, then tag

- Run the full check loop (`scripts/dev/round.sh`) and a full build
  (`meson compile -C build`) on the release commit.
- Tag that commit with the version (for example `0.2.0`) and push the tag.
  Releases come only from version tags.

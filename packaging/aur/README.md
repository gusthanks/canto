# AUR Packaging Templates (Not Published Yet)

These PKGBUILD templates prepare Canto for publishing to the Arch User Repository after a tagged GitHub release (`pkgver=0.1.0` or the chosen release version).

## Package naming

The AUR already has an unrelated EPUB reader (`canto-git`) providing `/usr/bin/canto`, and the Arch extra repository ships an RSS reader (`canto-curses` / `canto-daemon`). To avoid confusion while still installing `/usr/bin/canto`, the AUR packages are named:

- `canto-player`: builds from the release source tarball (`conflicts=('canto-git')`).
- `canto-player-bin`: installs a prebuilt `x86_64` release archive (`provides=('canto-player')`, `conflicts=('canto-player' 'canto-git')`).

## Before publishing a release to the AUR

1. Merge work into `main`, tag the release commit cleanly (`git tag <ver> && git push origin <ver>` — never push legacy tags).
2. Update `pkgver` (and maintainer email) in `PKGBUILD`, then update checksums:
   ```sh
   updpkgsums
   ```
3. Regenerate `.SRCINFO`:
   ```sh
   makepkg --printsrcinfo > .SRCINFO
   ```
4. Verify inside a clean Arch chroot before pushing:
   ```sh
   pkgctl build
   namcap *.pkg.tar.zst
   ```

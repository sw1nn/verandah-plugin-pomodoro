default:
    @just --list

# Bump the version, tag and push (see cog.toml), then build and upload packages.
release type='auto': && package
    cog bump --{{ type }}

# Build the Arch packages in a clean chroot and upload the current version.
package:
    #!/usr/bin/env bash
    # The version comes from the PKGBUILD rather than an argument, so this is
    # safe to re-run standalone after a failed or interrupted release.
    set -euo pipefail
    pkgver=$(sed -n 's/^pkgver=//p' packaging/arch/PKGBUILD)
    sw1nn-makepkg-chroot -C packaging/arch
    sw1nn-pkg-ctl upload packaging/arch/*-"$pkgver"-*.pkg.tar.zst

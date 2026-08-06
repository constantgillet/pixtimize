# Changelog

## [0.1.2](https://github.com/constantgillet/pixtimize/compare/v0.1.1...v0.1.2) (2026-08-06)


### Performance Improvements

* coalesce concurrent transform builds ([#19](https://github.com/constantgillet/pixtimize/issues/19)) ([e1ccdc1](https://github.com/constantgillet/pixtimize/commit/e1ccdc14449e47a9e77c404d39c64f982b64b78d))

## [0.1.1](https://github.com/constantgillet/pixtimize/compare/v0.1.0...v0.1.1) (2026-08-05)


### Features

* enforce ImageKit-compatible max size limits ([#7](https://github.com/constantgillet/pixtimize/issues/7)) ([f2ef4a3](https://github.com/constantgillet/pixtimize/commit/f2ef4a3ef56fb1773d666b9fb2225ebf6eb9f4f6))
* serve HEAD cache hits without downloading bodies ([#8](https://github.com/constantgillet/pixtimize/issues/8)) ([8012d61](https://github.com/constantgillet/pixtimize/commit/8012d6139480d9667cd0764c565693aa5838d1a4))


### Bug Fixes

* bump pinned Rust version to 1.90.0 ([#6](https://github.com/constantgillet/pixtimize/issues/6)) ([19ea093](https://github.com/constantgillet/pixtimize/commit/19ea0931afe4598fd7465e3b34dc58cee10ac03e))
* encode via libvips suffix options for version safety ([#15](https://github.com/constantgillet/pixtimize/issues/15)) ([d3ed69b](https://github.com/constantgillet/pixtimize/commit/d3ed69b5e4d5aa8e3077c4337043d25df7cdae5c))
* **nixpacks:** drop nixLibs to avoid glibc/vdso crash ([#12](https://github.com/constantgillet/pixtimize/issues/12)) ([7f54f39](https://github.com/constantgillet/pixtimize/commit/7f54f39ab67b625d2b33605ae54d7da0fdb76194))
* **nixpacks:** install libvips via apt and expose it to pkg-config ([4094162](https://github.com/constantgillet/pixtimize/commit/409416245406f07ad626e293d7d4f1740da765ff))
* **nixpacks:** install libvips via apt and expose it to pkg-config ([#10](https://github.com/constantgillet/pixtimize/issues/10)) ([635184e](https://github.com/constantgillet/pixtimize/commit/635184efc52201c2ac87da5756d0a9ca59a51eca))
* **nixpacks:** patchelf binary to use system linker for apt libs ([#14](https://github.com/constantgillet/pixtimize/issues/14)) ([1e73fb3](https://github.com/constantgillet/pixtimize/commit/1e73fb34f8d9439201d196c992e46a623645c3c2))
* **nixpacks:** remove LD_LIBRARY_PATH override that breaks nix bash ([#13](https://github.com/constantgillet/pixtimize/issues/13)) ([80100f8](https://github.com/constantgillet/pixtimize/commit/80100f8fe33db7470271dd35559d4e4a470cc32b))
* **nixpacks:** ship libvips/glib shared libs at runtime ([#11](https://github.com/constantgillet/pixtimize/issues/11)) ([80c4bfd](https://github.com/constantgillet/pixtimize/commit/80c4bfdc0c3f765604b8ea2f54f045f450483924))


### Performance Improvements

* replace image/webp pipeline with libvips ([#9](https://github.com/constantgillet/pixtimize/issues/9)) ([6d156bb](https://github.com/constantgillet/pixtimize/commit/6d156bb46b790bb1800f64cfd462bb47e4bb3ad1))

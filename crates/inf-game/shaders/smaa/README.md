# SMAA

The Vulkan renderer's SMAA anti-aliasing (`../smaa.glsl` and its three
`../smaa_*.comp` passes) runs the reference implementation of Enhanced
Subpixel Morphological Antialiasing by Jorge Jimenez, Jose I. Echevarria,
Belen Masia, Fernando Navarro and Diego Gutierrez, from
<https://github.com/iryoku/smaa> at commit
`71c806a838bdd7d517df19192a20f0c61b3ca29d` (2013-11-06), under its MIT
license, whose notice heads `SMAA.hlsl`.

- `SMAA.hlsl` is the reference file, byte for byte.
- `area.bin` and `search.bin` are the reference's precomputed tables
  (`Textures/AreaTex.h`, 160 by 560 texels of two bytes; `SearchTex.h`, 64
  by 16 of one byte), their array bytes in order, as the port uploads them.

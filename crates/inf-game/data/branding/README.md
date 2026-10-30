# Launcher artwork

`infestation-header-animation.png` is the pre-game launcher's complete
prerendered banner: new INFESTATION lettering over a thin line, against the
frontend's starfield and a still nebula. Once a loop a shine runs along the
line from left to right, in the menu logo's halo colour and brightness
curve, crossing in one ping cycle (64 ticks of the frontend's 20 ms clock).
The opaque 8-bit indexed PNG holds 192 frames in 16 columns and 12 rows;
each frame is 624 x 98 pixels, so the atlas is 9984 x 1176.
`header-animation.json` records that layout and the 20,000-microsecond
frame. One loop lasts 3.84 seconds.

Both files are written by
`build_header.py`;
its README has the
recovered timing and the recipe. They are compiled into `inf-game` and
decoded once before the launcher's window opens: about 11.2 MiB of palette
indices, one 256-colour palette and one cached BGRA frame. Nothing is
composited at runtime, and no retail installation, Python or OpenGL is
needed for it. The artwork reads no retail image, font, texture or mesh.

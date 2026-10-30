// The mirror's depth pyramid (vk_mirror_depth.comp, vk_mirror.comp): level
// 0 is the frame's TL depth with its one-pixel holes closed, and each level
// after it holds the nearest and farthest depths of the 2 by 2 cells below
// it. The sky and the water are no surface: nearest 1, farthest 0. The
// levels lie one after another in a buffer, each a row-major grid of its
// cells.

// A level's cells across and down, for a world drawn `size` pixels.
ivec2 level_size(ivec2 size, int level) {
    return max((size + (1 << level) - 1) >> level, ivec2(1));
}

// Where a level starts in the buffer.
int level_offset(ivec2 size, int level) {
    int at = 0;
    for (int below = 0; below < level; below++) {
        ivec2 cells = level_size(size, below);
        at += cells.x * cells.y;
    }
    return at;
}

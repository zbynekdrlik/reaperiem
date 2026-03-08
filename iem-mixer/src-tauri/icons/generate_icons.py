#!/usr/bin/env python3
"""Generate icons that EXACTLY match the tray icon design from tray.rs.

The tray icon is 16x16 with:
- Two circular ear cups at (4,10) and (12,10) with radius 3
- Headband arc at top (y depends on x)
- Vertical connectors at x=3 and x=13 from y=5-7
- Color: #4A9EFF (74, 158, 255)

This script generates scaled versions that preserve this exact design.
"""

from PIL import Image, ImageDraw
import struct
import io

# Exact color from tray.rs
BLUE = (0x4A, 0x9E, 0xFF)  # (74, 158, 255)
TRANSPARENT = (0, 0, 0, 0)

# Original design is 16x16
ORIGINAL_SIZE = 16


def draw_tray_icon_exact():
    """Draw the EXACT 16x16 tray icon design from tray.rs."""
    img = Image.new('RGBA', (16, 16), TRANSPARENT)

    def set_pixel(x, y):
        if 0 <= x < 16 and 0 <= y < 16:
            img.putpixel((x, y), BLUE + (255,))

    def draw_circle(cx, cy, r):
        for dy in range(r + 1):
            for dx in range(r + 1):
                if dx * dx + dy * dy <= r * r:
                    set_pixel(cx + dx, cy + dy)
                    set_pixel(cx + dx, cy - dy)
                    set_pixel(cx - dx, cy + dy)
                    set_pixel(cx - dx, cy - dy)

    # Left ear cup (circle at 4,10 with radius 3)
    draw_circle(4, 10, 3)

    # Right ear cup (circle at 12,10 with radius 3)
    draw_circle(12, 10, 3)

    # Headband arc at top
    for x in range(3, 14):
        if x < 5 or x > 11:
            y = 4
        elif x < 7 or x > 9:
            y = 3
        else:
            y = 2
        set_pixel(x, y)

    # Connect band to cups
    for y in [5, 6, 7]:
        set_pixel(3, y)
        set_pixel(13, y)

    return img


def draw_headphones_at_size(size):
    """Draw headphones at any size by scaling the design coordinates."""
    img = Image.new('RGBA', (size, size), TRANSPARENT)
    draw = ImageDraw.Draw(img)

    # Scale factor from 16x16
    s = size / 16.0

    # Draw filled circles for ear cups
    # Left ear cup (center 4,10 radius 3)
    left_cx, left_cy, left_r = 4 * s, 10 * s, 3 * s
    draw.ellipse([
        left_cx - left_r, left_cy - left_r,
        left_cx + left_r, left_cy + left_r
    ], fill=BLUE + (255,))

    # Right ear cup (center 12,10 radius 3)
    right_cx, right_cy, right_r = 12 * s, 10 * s, 3 * s
    draw.ellipse([
        right_cx - right_r, right_cy - right_r,
        right_cx + right_r, right_cy + right_r
    ], fill=BLUE + (255,))

    # Draw headband as thick arc
    band_thickness = max(1, int(s * 1.2))

    # Headband points (from tray.rs design)
    # x: 3-4,5-6,7-9,10-11,12-13 with y: 4,3,2,3,4
    points = []
    for x in range(3, 14):
        if x < 5 or x > 11:
            y = 4
        elif x < 7 or x > 9:
            y = 3
        else:
            y = 2
        points.append((x * s, y * s))

    # Draw band as thick line segments
    for i in range(len(points) - 1):
        draw.line([points[i], points[i + 1]], fill=BLUE + (255,), width=band_thickness)

    # Draw connectors (x=3 and x=13, y from 5-7)
    connector_width = max(1, int(s))
    # Left connector
    draw.rectangle([
        3 * s - connector_width / 2, 5 * s,
        3 * s + connector_width / 2, 7.5 * s
    ], fill=BLUE + (255,))
    # Right connector
    draw.rectangle([
        13 * s - connector_width / 2, 5 * s,
        13 * s + connector_width / 2, 7.5 * s
    ], fill=BLUE + (255,))

    return img


def create_multi_ico(images, output_path):
    """Create a multi-resolution ICO file."""
    header = struct.pack('<HHH', 0, 1, len(images))

    entries = []
    png_data = []
    offset = 6 + 16 * len(images)

    for img in images:
        png_buffer = io.BytesIO()
        img.save(png_buffer, format='PNG')
        data = png_buffer.getvalue()
        png_data.append(data)

        w, h = img.size
        w_byte = 0 if w == 256 else w
        h_byte = 0 if h == 256 else h

        entry = struct.pack('<BBBBHHII',
            w_byte, h_byte, 0, 0, 1, 32, len(data), offset
        )
        entries.append(entry)
        offset += len(data)

    with open(output_path, 'wb') as f:
        f.write(header)
        for entry in entries:
            f.write(entry)
        for data in png_data:
            f.write(data)


def main():
    print("Generating icons that match tray.rs design exactly...")

    # Generate base 16x16 icon (exact match to tray icon)
    base = draw_tray_icon_exact()
    base.save('16x16-base.png', 'PNG')
    print("✓ Created 16x16-base.png (reference)")

    # Generate icons at each size directly (not scaled)
    sizes = {
        '32x32.png': 32,
        '128x128.png': 128,
        'icon.png': 512,
    }

    for filename, size in sizes.items():
        img = draw_headphones_at_size(size)
        img.save(filename, 'PNG')
        print(f"✓ Created {filename}")

    # Generate multi-resolution ICO - draw each size directly
    ico_sizes = [16, 24, 32, 48, 64, 128, 256]
    ico_images = []
    for s in ico_sizes:
        if s == 16:
            ico_images.append(draw_tray_icon_exact())
        else:
            ico_images.append(draw_headphones_at_size(s))
    create_multi_ico(ico_images, 'headphones.ico')
    print(f"✓ Created headphones.ico ({len(ico_sizes)} sizes)")

    print("\nDone! Icons now match tray.rs design.")


if __name__ == "__main__":
    main()

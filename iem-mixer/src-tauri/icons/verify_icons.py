#!/usr/bin/env python3
"""Verify icon files have correct headphones shape with proper transparency.

This test ensures:
1. Center area is FULLY transparent (alpha=0) - gap between ear cups
2. Ear areas are blue (#4A9EFF) with full opacity
3. Corners are FULLY transparent (alpha=0) - not a solid rectangle

Run: python3 verify_icons.py
"""

from PIL import Image
import sys

BLUE = (74, 158, 255)  # #4A9EFF
TOLERANCE = 10  # Color tolerance


def verify_icon(path: str) -> list[str]:
    """Verify an icon file. Returns list of errors (empty if valid)."""
    errors = []

    try:
        img = Image.open(path).convert("RGBA")
    except Exception as e:
        return [f"Failed to open {path}: {e}"]

    w, h = img.size

    # Check center pixel (should be transparent - gap between ear cups)
    cx, cy = w // 2, h // 2
    center = img.getpixel((cx, cy))
    if center[3] != 0:
        errors.append(f"Center ({cx},{cy}) not transparent: alpha={center[3]}, expected 0")

    # Check all 4 corners (should be transparent)
    corners = [(0, 0), (w-1, 0), (0, h-1), (w-1, h-1)]
    for x, y in corners:
        pixel = img.getpixel((x, y))
        if pixel[3] != 0:
            errors.append(f"Corner ({x},{y}) not transparent: alpha={pixel[3]}, expected 0")

    # Check ear areas (should be blue with full opacity)
    # Left ear at ~22% from left, ~62% from top
    left_ear_x = int(w * 0.22)
    left_ear_y = int(h * 0.62)
    left = img.getpixel((left_ear_x, left_ear_y))

    if left[3] != 255:
        errors.append(f"Left ear ({left_ear_x},{left_ear_y}) not opaque: alpha={left[3]}")
    elif not (abs(left[0] - BLUE[0]) <= TOLERANCE and
              abs(left[1] - BLUE[1]) <= TOLERANCE and
              abs(left[2] - BLUE[2]) <= TOLERANCE):
        errors.append(f"Left ear ({left_ear_x},{left_ear_y}) not blue: RGB=({left[0]},{left[1]},{left[2]})")

    # Right ear at ~78% from left, ~62% from top
    right_ear_x = int(w * 0.78)
    right_ear_y = int(h * 0.62)
    right = img.getpixel((right_ear_x, right_ear_y))

    if right[3] != 255:
        errors.append(f"Right ear ({right_ear_x},{right_ear_y}) not opaque: alpha={right[3]}")
    elif not (abs(right[0] - BLUE[0]) <= TOLERANCE and
              abs(right[1] - BLUE[1]) <= TOLERANCE and
              abs(right[2] - BLUE[2]) <= TOLERANCE):
        errors.append(f"Right ear ({right_ear_x},{right_ear_y}) not blue: RGB=({right[0]},{right[1]},{right[2]})")

    return errors


def main():
    icons = ["32x32.png", "128x128.png", "icon.png"]
    all_errors = []

    print("Verifying icon files...")
    print()

    for icon in icons:
        errors = verify_icon(icon)
        if errors:
            print(f"❌ {icon}:")
            for e in errors:
                print(f"   - {e}")
            all_errors.extend(errors)
        else:
            print(f"✓ {icon}: OK")

    print()

    if all_errors:
        print(f"FAILED: {len(all_errors)} error(s) found")
        print("Icons must show headphones shape with transparent background,")
        print("NOT a solid blue rectangle.")
        sys.exit(1)
    else:
        print("PASSED: All icons are valid headphones shape")
        sys.exit(0)


if __name__ == "__main__":
    main()

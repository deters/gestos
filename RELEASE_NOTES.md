# Gestos 6.1.1

## Fixed

Toggle Tile Windows, in the “Gestos” GNOME extension:

- Windows no longer cover each other when an app can't get as small as its tile, sets its smallest size late, or resizes its window itself. Apps that keep resizing their window keep that size, still tiled, and the tiles beside them take the rest.
- Tiles stay at least 100 pixels wide and tall while the screen has room for them.
- The gap between tiles is exact when it's an odd number of pixels.
- Windows moved to a tiled workspace join once they're on screen, so GNOME's placement doesn't undo their tile.

Log out and back in after updating, so GNOME loads the new extension.

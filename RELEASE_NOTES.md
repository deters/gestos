# Gestos 6.2.2

## Fixed

- **Windows that misreport their smallest size tile like the others.** Some apps, like ones built on WebKit, say the size they open at is the smallest they can get, which left them overlapping their neighbors. A window that says it can't get smaller than half the screen now gets a tile like the others.
- **Tiles stay usable.** No tile gets smaller than a quarter of the screen's width or a third of its height, so up to 12 windows fit, 4 across and 3 down.

Log out and back in after updating, so GNOME loads the new extension.

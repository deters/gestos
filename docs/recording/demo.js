// Paste into the Broadway page. It draws a pointer, since Broadway shows none, and sends mouse
// events. Coordinates are page pixels at a 720x720 viewport.
document.documentElement.style.overflow = "hidden";
document.body.style.overflow = "hidden";

const pause = (ms) => new Promise((done) => setTimeout(done, ms));

window.demo = (() => {
  const pointer = document.createElement("div");
  pointer.style.cssText =
    "position:fixed;left:0;top:0;width:28px;height:28px;z-index:2147483647;pointer-events:none;";
  pointer.innerHTML =
    '<svg width="28" height="28" viewBox="0 0 24 24"><path d="M3 2 L3 19 L8 14.5 L11.5 22 L14.5 20.7 L11 13.5 L18 13.5 Z" fill="#111" stroke="#fff" stroke-width="1.5" stroke-linejoin="round"/></svg>';
  document.documentElement.appendChild(pointer);

  let x = 0;
  let y = 0;
  let held = false;
  const send = (type, buttons) => {
    const target = document.elementFromPoint(x, y) || document.body;
    target.dispatchEvent(
      new MouseEvent(type, {
        bubbles: true,
        cancelable: true,
        clientX: x,
        clientY: y,
        screenX: x,
        screenY: y,
        button: 0,
        buttons,
      }),
    );
  };

  return {
    at(newX, newY) {
      const before = document.elementFromPoint(x, y);
      x = newX;
      y = newY;
      pointer.style.transform = `translate(${x}px,${y}px)`;
      if (document.elementFromPoint(x, y) !== before) send("mouseover", 0);
      send("mousemove", held ? 1 : 0);
    },
    down() {
      held = true;
      send("mousedown", 1);
    },
    up() {
      held = false;
      send("mouseup", 0);
    },
    click(newX, newY) {
      this.at(newX, newY);
      this.down();
      this.up();
    },
    // Moves one point at a time, because Broadway merges moves sent together into straight lines.
    async trace(points) {
      for (const [pointX, pointY] of points) {
        this.at(pointX, pointY);
        await pause(45);
      }
    },
  };
})();

// The two paths, as points to trace after demo.down().
window.upPath = [];
for (let y = 470; y >= 215; y -= 8) window.upPath.push([310, y]);

window.sPath = [];
const [cx, cy, r] = [310, 340, 70];
for (let t = -30; t >= -270; t -= 8) {
  const a = (t * Math.PI) / 180;
  window.sPath.push([cx + r * Math.cos(a), cy - r + r * Math.sin(a)]);
}
for (let t = -90; t <= 150; t += 8) {
  const a = (t * Math.PI) / 180;
  window.sPath.push([cx + r * Math.cos(a), cy + r + r * Math.sin(a)]);
}

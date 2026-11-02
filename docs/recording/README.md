# Recording the README's GIFs

`docs/screenshots/maximize-gesture.gif` and `save-gesture.gif` show setting up the first two gestures. **When that part of the README needs updating, re-record them with this script**, so that they keep showing the same story on the current screens.

## The script

One run gives both GIFs. Step 11 is the last frame of the first and the first of the second.

| Step | Shows | Hold (1/100 s) |
|---|---|---|
| 1 | The empty app, listing every action | 130 |
| 2 | Typing `max` | 70 |
| 3 | The full `maximize` | 110 |
| 4 | Pointer on **Toggle Maximized** | 90 |
| 5 | The empty editor, pointer on the canvas | 90 |
| 6–7 | Drawing a line **upward** | 40 each |
| 8 | The finished line | 100 |
| 9 | Pointer on **Save** | 70 |
| 10 | The gesture saved, search still filled | 130 |
| 11 | Search cleared, pointer away | 400 ending `maximize-gesture`, 130 starting `save-gesture` |
| 12 | Typing `sa` | 70 |
| 13 | The full `save` | 100 |
| 14 | Pointer on **Save (Ctrl+S)** | 80 |
| 15 | The empty editor | 90 |
| 16–18 | Drawing an **S** | 40 each |
| 19 | The finished S | 110 |
| 20 | Pointer on **Save** | 70 |
| 21 | The gesture saved, pointer on the search's clear button | 110 |
| 22 | **Search cleared, both gestures showing, pointer away** | 400 |

Each GIF ends with the search box cleared, so that its new gesture can be seen. If the wording in the README describes these gestures, keep the two in step.

## Recording it

Everything runs isolated: see the top of [record.sh](record.sh). Build first, with `make` or `cargo build`, then:

1. Start the server, and leave it running:

   ```bash
   docs/recording/record.sh server
   ```

2. Open `http://127.0.0.1:8089/` in a browser, with a 720x720 viewport. **Do this before the next step:** a page that connects after the app has drawn stays blank.

3. Start the app:

   ```bash
   docs/recording/record.sh app
   ```

4. Load [demo.js](demo.js) into the page, then go through the script, taking a screenshot for each step.

5. Build each GIF, from its name and each screenshot and how long it holds:

   ```bash
   docs/recording/record.sh build maximize-gesture step1.jpg:130 step2.jpg:70
   ```

   ```bash
   docs/recording/record.sh build save-gesture step11.jpg:130 step12.jpg:70
   ```

6. Stop everything, which also deletes the temporary home folder:

   ```bash
   docs/recording/record.sh stop
   ```

## Things to get right

Positions are page pixels at a 720x720 viewport, and may need adjusting when the screens change:

| Target | Position |
|---|---|
| Search box | 300, 85 |
| Search's clear button | 596, 85 |
| Save button in the editor | 612, 34 |
| Resting place for the pointer | 620, 650 |
| First row of results | 290, 238 |
| Row spacing | about 48 |

- **Typing:** send real key presses, one letter at a time. Pasted or inserted text doesn't reach Broadway. Click the search box first if it doesn't have focus.
- **Drawing:** use `demo.down()`, then `await demo.trace(upPath)` or `sPath`, split across steps by slicing the array, then `demo.up()`. Sending moves all at once turns curves into zigzags.
- **Waiting:** Broadway lags behind, so allow a second or two after a click before taking a screenshot, or it can show the previous screen.
- **Cropping:** `build` crops to the window at 549x549 from 10,9, which assumes screenshots of the 720x720 viewport at 800x600. Adjust it if the browser scales differently.
- **Not shown:** gestures aren't run, because the daemon is a stand-in. The app is real, and so is what it saves.

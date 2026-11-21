# Principles

What Canto is trying to be, and the rules that follow. Read it before adding
a feature or changing a look; `doc/interface-map.md` has each area's decisions.

## Who it is for

People who make their desktop their own and want their music in one place:
"Your music, your way. Everything in one place." Every new feature passes one
filter: does it serve someone who wants everything in one place, arranged
their way? Personalization can grow into an endless list of options, so until
the first release it rests on two pillars only.

1. **Colors follow the system.** "Follow System" is the first theme. On
   Omarchy it reads the current theme's colors and accent and changes live
   when the desktop's theme changes. Other desktops get the fixed Green
   palette for now; following them is still to do. The fixed themes are a
   choice, never the default.
2. **A player bar each user arranges.** Every optional button can be shown,
   hidden and moved ("Customize Bar…", or a right click on the bar). The gear
   never leaves, so there is always a way back.

## One control per function

- Each function has one control in one place. If a second place seems
  needed, the first one is probably in the wrong spot.
- Controls used rarely live inside a button with a popover: the lyrics'
  timing, the sleep timer, the equalizer. The bars hold icons only.
- The name and the state go in the tooltip. A state out of the ordinary
  lights the icon in the accent color (sound on another device, lyrics
  shifted in time).
- Controls that work together are one piece (a linked capsule, such as
  translate and its language), not two loose buttons.
- A summary opens its full view instead of repeating it: a card in Now
  Playing opens its tab.

## Icons, images and branding

- Every function has its own icon. Two identical icons for different
  functions on one screen (two "⋯") confuse, so one of them changes.
- Symbolic icons are filled shapes, no strokes, so GTK can recolor them.
  They are drawn for Canto, bundled with it, and carry no brand logos.
- Canto is an unofficial client: no Spotify logo or name in its own
  branding. The green theme is called "Green".
- Images are sharp and backgrounds solid, never a small picture blown up
  and blurred. Nothing sits on top of the content (no thick scrollbar over
  the rows, no handle on the page's edge).

## Nothing that puts the account at risk

- Canto never does what could get a listener's Spotify account flagged: no
  downloads, no lyrics pulled from Spotify through librespot, no copying from
  sites without their permission.
- Anything that writes to an account or publishes for the listener (loving
  a song on Last.fm, sending lyrics to LRCLIB) has its own switch or asks to
  confirm, and says that the result is public when it is.
- Keys the listener brings (Last.fm, Genius and others) stay on their
  machine. Logs never hold a token.

## Small windows

Canto has to work tiled at half a laptop screen as well as full screen:

- Design from the narrowest window up (360 px). Every width between the
  steps must work, not only the ones that were looked at.
- Drop parts in steps rather than squeezing them: the player bar stays on one
  line, the title stays readable, and buttons leave in a fixed order. What
  leaves stays reachable where it can (More Options, Ctrl+K, the side
  panel's tabs).
- Panels float over the page before they crush it: under 1100 sp the side
  panel is a drawer, and beside it the library shrinks to its rail of covers
  so the page keeps room for an album's header.
- One thing over the page at a time, closed with Esc or a click outside, and
  the keyboard focus comes back where it was without moving the page.
- Text scale is part of the width: a step in sp moves with larger text, while
  icons keep their px.
- Nothing may be cut: a layout that does not fit is a bug, and the GTK tests
  fail on any "exceeds" or negative allocation warning.
- Fractional scales and 1 px lines show only on a real screen: look there
  before calling a layout done.

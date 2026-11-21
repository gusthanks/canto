#!/usr/bin/env python3
"""Drive a running Canto through the accessibility tree (AT-SPI), for manual checks.

Usage: scripts/dev/atspi.py COMMAND [ARGS]
  dump [TEXT]             list the showing nodes (index, role, name, actions),
                          only those whose role or name contains TEXT
  act INDEX [ACTION]      run an action (default: the first) on node INDEX from dump
  click ROLE NAME [NTH]   click the NTH (default 0) clickable node with that role and name
  scrolltop               move every scroll bar to the top
  focus INDEX             give node INDEX the keyboard focus

It acts on the real app, in the real session: it clicks what you tell it to.
CANTO_ATSPI_APP names the application to look for (default: canto).
Needs PyGObject and the AT-SPI 2 typelib.
"""
import os
import sys

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402

APP_NAME = os.environ.get("CANTO_ATSPI_APP", "canto")


def app():
    desktop = Atspi.get_desktop(0)
    for i in range(desktop.get_child_count()):
        child = desktop.get_child_at_index(i)
        if child is not None and child.get_name() == APP_NAME:
            return child
    sys.exit(f"no running application named {APP_NAME!r} on the accessibility bus")


def walk(node, depth=0, out=None, max_depth=60):
    if out is None:
        out = []
    try:
        role = node.get_role_name()
        name = node.get_name() or ""
        showing = node.get_state_set().contains(Atspi.StateType.SHOWING)
    except Exception:
        return out
    if not showing and depth > 2:
        return out
    out.append((depth, role, name, node))
    if depth < max_depth:
        for i in range(node.get_child_count()):
            child = node.get_child_at_index(i)
            if child is not None:
                walk(child, depth + 1, out, max_depth)
    return out


def nodes():
    return walk(app())


def actions(node):
    try:
        return [node.get_action_name(i) for i in range(node.get_n_actions())]
    except Exception:
        return []


def main(argv):
    if len(argv) < 2:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    cmd = argv[1]
    if cmd == "dump":
        text = argv[2].lower() if len(argv) > 2 else ""
        for k, (depth, role, name, node) in enumerate(nodes()):
            if text in (role + " " + name).lower():
                print(k, "  " * min(depth, 20) + role, repr(name[:70]), actions(node))
    elif cmd == "act":
        node = nodes()[int(argv[2])][3]
        acts = actions(node)
        i = acts.index(argv[3]) if len(argv) > 3 else 0
        print("do", acts[i], node.do_action(i))
    elif cmd == "click":
        role, name = argv[2], argv[3]
        nth = int(argv[4]) if len(argv) > 4 else 0
        hits = [n for (_, r, nm, n) in nodes() if r == role and nm == name and "click" in actions(n)]
        if nth >= len(hits):
            sys.exit(f"found {len(hits)} clickable {role} named {name!r}")
        node = hits[nth]
        print("click", role, name, node.do_action(actions(node).index("click")))
    elif cmd == "scrolltop":
        for (_, role, _, node) in nodes():
            if role == "scroll bar":
                try:
                    Atspi.Value.set_current_value(node, 0.0)
                    print("top")
                except Exception as e:
                    print("could not scroll:", e)
    elif cmd == "focus":
        node = nodes()[int(argv[2])][3]
        print(Atspi.Component.grab_focus(node))
    else:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))

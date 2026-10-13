"""Driver-matrix fixture, GTK 3: a button, a checkbox and an empty entry.
The window title reports their state ("kai-fixture gtk3 | clicks=1
checked=1 name=Asha"), so the matrix checks what really changed through the
X window title, not through the accessibility tree it is testing."""
import gi

gi.require_version("Gtk", "3.0")
from gi.repository import Gtk  # noqa: E402

state = {"clicks": 0}
win = Gtk.Window(title="kai-fixture gtk3")
box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12, margin=24)
button = Gtk.Button(label="Apply")
check = Gtk.CheckButton(label="Remember me")
entry = Gtk.Entry()
entry.get_accessible().set_name("Name")


def show(*_):
    win.set_title(
        f"kai-fixture gtk3 | clicks={state['clicks']} "
        f"checked={int(check.get_active())} name={entry.get_text()}"
    )


def clicked(_):
    state["clicks"] += 1
    show()


button.connect("clicked", clicked)
check.connect("toggled", show)
entry.connect("changed", show)
for w in (button, check, entry):
    box.pack_start(w, False, False, 0)
win.add(box)
win.set_default_size(360, 220)
win.connect("destroy", Gtk.main_quit)
show()
win.show_all()
Gtk.main()

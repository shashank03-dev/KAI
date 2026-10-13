"""Driver-matrix fixture, Qt 5: the same three widgets as gtk3.py, with the
state in the window title."""
import sys

from PyQt5 import QtWidgets

app = QtWidgets.QApplication(sys.argv)
win = QtWidgets.QWidget()
layout = QtWidgets.QVBoxLayout(win)
button = QtWidgets.QPushButton("Apply")
check = QtWidgets.QCheckBox("Remember me")
entry = QtWidgets.QLineEdit()
entry.setAccessibleName("Name")
state = {"clicks": 0}


def show(*_):
    win.setWindowTitle(
        f"kai-fixture qt5 | clicks={state['clicks']} "
        f"checked={int(check.isChecked())} name={entry.text()}"
    )


def clicked():
    state["clicks"] += 1
    show()


button.clicked.connect(clicked)
check.toggled.connect(show)
entry.textChanged.connect(show)
for w in (button, check, entry):
    layout.addWidget(w)
win.resize(360, 220)
show()
win.show()
sys.exit(app.exec_())

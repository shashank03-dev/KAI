# Print every mesh part's world-space box and flag parts that touch nothing.
# Run: blender --background FILE.blend --python inspect.py
import bpy
from mathutils import Vector

def box(o):
    pts = [o.matrix_world @ Vector(c) for c in o.bound_box]
    lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    return lo, hi

def touch(a, b, tol=0.01):
    return all(a[0][i] <= b[1][i] + tol and b[0][i] <= a[1][i] + tol for i in range(3))

parts = [o for o in bpy.data.objects if o.type == 'MESH']
boxes = {o.name: box(o) for o in parts}
print("KAI-INSPECT parts:", len(parts))
for n, (lo, hi) in boxes.items():
    size = hi - lo
    print(f"KAI-INSPECT {n}: size {size.x:.2f} x {size.y:.2f} x {size.z:.2f} m, "
          f"x {lo.x:.2f}..{hi.x:.2f}, y {lo.y:.2f}..{hi.y:.2f}, z {lo.z:.2f}..{hi.z:.2f}")
alone = [n for n in boxes if not any(touch(boxes[n], boxes[m]) for m in boxes if m != n)]
if len(parts) > 1 and alone:
    print("KAI-INSPECT PROBLEM: these parts touch no other part (floating or misplaced):", ", ".join(alone))
else:
    print("KAI-INSPECT OK: every part touches another part")

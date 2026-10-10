---
name: blender-modeling
description: Make or change a 3D model in Blender (shapes, simple furniture, objects, materials, renders) and save a .blend file. Use when the task mentions Blender or 3D modelling.
license: MIT
metadata:
  author: kai
  version: "1.1"
---
# Blender: model with Python, not with the mouse

Blender draws its own interface, so there is no accessibility tree and
clicking through it is unreliable. Build the model with Blender's Python API
(`bpy`), run it headless, check it with the helper scripts, then show it.

1. Write the script in ONE `shell` step with a heredoc:
   `mkdir -p ~/kai-bench && cat > ~/kai-bench/model.py <<'PY'` … `PY`.
   - Start empty: `import bpy, os; bpy.ops.wm.read_factory_settings(use_empty=True)`.
   - Sizes: `primitive_cube_add(size=1, location=(x, y, z))` then
     `obj.scale = (W, D, H)` gives a box W x D x H metres centred on (x, y, z),
     so it spans x-W/2..x+W/2. A cylinder `primitive_cylinder_add(radius=r,
     depth=h, location=(x, y, z))` spans z-h/2..z+h/2.
   - Parts must touch: put legs INSIDE the top's footprint (for a top W x D,
     legs at x = ±(W/2 - r - 0.05), y = ±(D/2 - r - 0.05)), from the floor
     (z = 0) up to the underside of the top.
   - Name parts (`obj.name = "Leg 1"`) and give them a material
     (`bpy.data.materials.new`, `use_nodes = True`, Principled BSDF "Base Color").
   - End with `bpy.ops.wm.save_as_mainfile(filepath=os.path.expanduser("~/…/name.blend"))`.
2. Run it: `blender --background --python ~/kai-bench/model.py 2>&1 | tail -3`.
3. Inspect it (sizes, positions, and parts that touch nothing):
   `blender --background ~/…/name.blend --python {skill_dir}/scripts/inspect.py 2>&1 | grep KAI-`
   If it prints `PROBLEM`, fix the numbers in model.py and go back to step 2.
4. Look at it: render a preview and open it so you can see it on screen:
   `blender --background ~/…/name.blend --python {skill_dir}/scripts/preview.py -- ~/kai-bench/preview.png 2>&1 | grep KAI-`
   then `display-im6.q16 ~/kai-bench/preview.png` (Windows: `start ~/kai-bench/preview.png`).
   Check the picture matches the request; fix and repeat if not.
5. Reply `done` with what was built, its size, and where it was saved.

Never delete or overwrite a user's existing .blend without asking.

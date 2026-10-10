# Render a quick preview PNG of the scene (CPU, no GPU needed).
# Run: blender --background FILE.blend --python preview.py -- OUT.png
import bpy, sys, math
from mathutils import Vector
out = sys.argv[sys.argv.index("--") + 1]
scn = bpy.context.scene
scn.render.engine = 'CYCLES'
scn.cycles.device = 'CPU'
scn.cycles.samples = 16
scn.cycles.use_denoising = False
scn.render.resolution_x, scn.render.resolution_y = 800, 500
meshes = [o for o in bpy.data.objects if o.type == 'MESH']
pts = [o.matrix_world @ Vector(c) for o in meshes for c in o.bound_box] or [Vector((0, 0, 0))]
centre = sum(pts, Vector()) / len(pts)
radius = max((p - centre).length for p in pts) or 1.0
sun = bpy.data.objects.new("kai_sun", bpy.data.lights.new("kai_sun", 'SUN'))
sun.data.energy = 4
sun.rotation_euler = (0.7, 0.2, 0.5)
scn.collection.objects.link(sun)
if scn.world is None:
    scn.world = bpy.data.worlds.new("kai_world")
scn.world.color = (0.8, 0.8, 0.85)
cam = bpy.data.objects.new("kai_cam", bpy.data.cameras.new("kai_cam"))
scn.collection.objects.link(cam)
d = radius * 3.2
cam.location = centre + Vector((d * 0.6, -d * 0.7, d * 0.5))
cam.rotation_euler = (centre - cam.location).to_track_quat('-Z', 'Y').to_euler()
scn.camera = cam
scn.render.filepath = out
bpy.ops.render.render(write_still=True)
print("KAI-PREVIEW saved", out)

"""Held gear library (pure Python, no Blender; docs/assets-humans.md, "Held gear"): the
weapons, instruments and colours the families put in their men's hands, as plain mesh data
that `gear.held` turns into an object on a socket.

Item frame (every item): the origin is the grip of the hand that holds it (the drum: its
carriage hook; the sling: the finger loop; the shield: the hand grip behind the boss), the
long axis is +Z (point, head or muzzle up), the edge, blade or face looks -Y. `gear.held`
puts this frame on a socket bone as `gear.musket` is put (upright in the socket's rest
frame); the clips turn it.

Each item is an `Item`: vertices (m), faces with a part per face, the material key of each
part (`KEYS`, a family may remap them), named sockets (empties under the object:
`socket.tip` at the point of a melee weapon, docs/melee.md "Reach"; `socket.muzzle` at a
firearm's muzzle; others where a clip needs a target, e.g. the drum's head, the bow's nock)
and vertex groups (`cloth`, `pin`: the colour's cloth and its hoist, for the runtime cloth
later). `BUDGET` caps each item's triangles; `ITEMS` lists the builders.

Shapes follow period pieces: the British infantry side drum and its colours (Clothing Warrant
1751: colours 6 ft 6 in by 6 ft on a 9 ft 10 in pike with a spear-point finial), the
smallsword and brass-hilted hanger of the 1750s, the sergeant's halberd and the officer's
spontoon, Woodlands self bows, the ball-headed club and trade tomahawk (Johnson & Hook, MAA
228), the macuahuitl after the Codex Mendoza, the Andean sling (Guaman Poma), a hide
chimalli-like round shield and a sea-service flintlock pistol.
"""

import math

from . import registry

# Material keys the items use beyond the shared ones (registered for every family).
registry.material("vellum", "leather", fallback=(0.86, 0.80, 0.66))
registry.material("cord", "linen", fallback=(0.80, 0.74, 0.60))
registry.material("colour", "linen", fallback=(0.70, 0.12, 0.10))
registry.material("hide", "hide", fallback=(0.62, 0.48, 0.32))
registry.material("stone", "bone", fallback=(0.10, 0.10, 0.12))
registry.material("feather", "hair", fallback=(0.85, 0.82, 0.76))


class Item:
    """Mesh data of one held item: `verts` [(x, y, z)], `faces` [(i, ...)], `parts` (one
    part name per face), `keys` {part: material key}, `sockets` {name: (x, y, z)},
    `groups` {name: [vertex index]}."""

    def __init__(self, name):
        self.name = name
        self.verts = []
        self.faces = []
        self.parts = []
        self.keys = {}
        self.sockets = {}
        self.groups = {}

    # -- primitives ------------------------------------------------------------------------

    def v(self, co):
        self.verts.append(tuple(float(c) for c in co))
        return len(self.verts) - 1

    def f(self, idx, part):
        self.faces.append(tuple(idx))
        self.parts.append(part)

    def ring(self, centre, radius, n, frame=None, scale=(1.0, 1.0), phase=0.0):
        """`n` vertices round `centre` in the plane of `frame` = (u, w) unit vectors (default
        the XY plane), radii `radius * scale`."""
        u, w = frame or ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0))
        out = []
        for i in range(n):
            a = 2 * math.pi * (i + phase) / n
            cu, cw = math.cos(a) * radius * scale[0], math.sin(a) * radius * scale[1]
            out.append(self.v(tuple(c + cu * a1 + cw * b1 for c, a1, b1 in zip(centre, u, w))))
        return out

    def bridge(self, a, b, part):
        n = len(a)
        for i in range(n):
            j = (i + 1) % n
            self.f((a[i], a[j], b[j], b[i]), part)

    def cap(self, ring, part, flip=False):
        self.f(tuple(reversed(ring)) if flip else tuple(ring), part)

    def tube(self, path, part, n=6, caps=True, scale=(1.0, 1.0)):
        """A tube through `path` [((x, y, z), radius)]; radius 0 closes to a point."""
        rings = []
        for k, (c, r) in enumerate(path):
            a = path[max(k - 1, 0)][0]
            b = path[min(k + 1, len(path) - 1)][0]
            t = _norm(_sub(b, a))
            u = _norm(_cross(t, (0.0, 1.0, 0.0) if abs(t[1]) < 0.9 else (1.0, 0.0, 0.0)))
            w = _cross(t, u)
            rings.append([self.v(c)] if r == 0 else self.ring(c, r, n, (u, w), scale))
        for a, b in zip(rings, rings[1:]):
            if len(a) == 1 and len(b) == 1:
                continue
            if len(a) == 1:
                for i in range(n):
                    self.f((a[0], b[i], b[(i + 1) % n]), part)
            elif len(b) == 1:
                for i in range(n):
                    self.f((a[(i + 1) % n], a[i], b[0]), part)
            else:
                self.bridge(a, b, part)
        if caps:
            if len(rings[0]) > 1:
                self.cap(rings[0], part, flip=True)
            if len(rings[-1]) > 1:
                self.cap(rings[-1], part)
        return rings

    def box(self, centre, size, part, taper=(1.0, 1.0), shift=(0.0, 0.0)):
        """A box (`size` x, y, z) round `centre`; the top face scaled by `taper` and moved
        by `shift` (x, y)."""
        cx, cy, cz = centre
        sx, sy, sz = (s / 2 for s in size)
        idx = []
        for z in (-1, 1):
            for x, y in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
                k = taper if z > 0 else (1.0, 1.0)
                d = shift if z > 0 else (0.0, 0.0)
                idx.append(self.v((cx + x * sx * k[0] + d[0], cy + y * sy * k[1] + d[1], cz + z * sz)))
        b, t = idx[:4], idx[4:]
        self.f(tuple(reversed(b)), part)
        self.f(tuple(t), part)
        self.bridge(b, t, part)
        return idx

    def blade(self, z0, length, width, thick, part, points=4, curve=0.0, edge="both"):
        """A flat blade from z0 up `length`: a diamond section (`edge` "both") or a back and
        one edge (-Y, "one"), narrowing to a point; `curve` bends the point towards +Y (m)."""
        rings = []
        for k in range(points + 1):
            t = k / points
            z = z0 + t * length
            dy = curve * t * t
            if k == points:
                rings.append([self.v((0.0, -width * 0.15 + dy, z + width * 0.25))])
                break
            w = width * (1.0 - 0.55 * t)
            h = thick * (1.0 - 0.5 * t) / 2
            if edge == "both":
                pts = [(0.0, -w / 2), (h, 0.0), (0.0, w / 2), (-h, 0.0)]
            else:
                pts = [(0.0, -w / 2), (h, w / 2), (-h, w / 2)]
            rings.append([self.v((x, y + dy, z)) for x, y in pts])
        for a, b in zip(rings, rings[1:]):
            if len(b) == 1:
                for i in range(len(a)):
                    self.f((a[i], a[(i + 1) % len(a)], b[0]), part)
            else:
                self.bridge(a, b, part)
        self.cap(rings[0], part, flip=True)
        return rings

    def strip(self, pts, width, part, normal=(1.0, 0.0, 0.0)):
        """A flat ribbon through `pts`, `width` across `normal` x direction (cords, strings)."""
        left, right = [], []
        for k, p in enumerate(pts):
            a, b = pts[max(k - 1, 0)], pts[min(k + 1, len(pts) - 1)]
            side = _norm(_cross(_norm(_sub(b, a)), normal))
            left.append(self.v(tuple(c + s * width / 2 for c, s in zip(p, side))))
            right.append(self.v(tuple(c - s * width / 2 for c, s in zip(p, side))))
        for i in range(len(pts) - 1):
            self.f((left[i], right[i], right[i + 1], left[i + 1]), part)

    # -- queries ---------------------------------------------------------------------------

    def triangles(self):
        return sum(len(f) - 2 for f in self.faces)

    def bounds(self):
        xs = list(zip(*self.verts))
        return tuple(min(c) for c in xs), tuple(max(c) for c in xs)

    def material_keys(self):
        """The material keys in slot order (`acr_material_keys`) and each face's slot."""
        order = list(dict.fromkeys(self.parts))
        keys = list(dict.fromkeys(self.keys[p] for p in order))
        return keys, [keys.index(self.keys[p]) for p in self.parts]


def set_materials(mesh, mats):
    """Replaces the material slots of Blender mesh `mesh` by `mats`, keeping each face's
    slot. `materials.clear()` alone drops the mesh's `material_index` attribute, so every
    face of a multi-material item (the drum's hoops, heads and cords, the musket's lock)
    would fall back to slot 0 (#287)."""
    slots = [p.material_index for p in mesh.polygons] if len(mats) > 1 else None
    mesh.materials.clear()
    for m in mats:
        mesh.materials.append(m)
    if slots is not None:
        for p, s in zip(mesh.polygons, slots):
            p.material_index = min(s, len(mats) - 1)


def _sub(a, b):
    return tuple(x - y for x, y in zip(a, b))


def _cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def _norm(a):
    n = math.sqrt(sum(x * x for x in a)) or 1.0
    return tuple(x / n for x in a)


# ----------------------------------------------------------------------------- the items

def drum(radius=0.205, depth=0.36):
    """An 18th-century side drum: a deep wooden shell (painted in the facings colour by the
    family), two hoops, vellum heads and the rope tensioning cords zigzagging between the
    hoops. Origin at the carriage hook on the shell (the drum hangs from the belt on the
    left thigh), the drum's axis along Z; `socket.head` the centre of the batter head
    (where the sticks strike)."""
    it = Item("drum")
    it.keys = {"shell": "facings", "hoop": "wood", "head": "vellum", "cord": "cord"}
    n = 16
    off = (0.0, radius, 0.0)  # the hook on the shell's back (+Y)

    def at(x, y, z):
        return (x - off[0], y - off[1], z - off[2])

    shell = [it.ring(at(0, 0, z), radius, n) for z in (-depth / 2 + 0.04, depth / 2 - 0.04)]
    it.bridge(shell[0], shell[1], "shell")
    for s in (-1, 1):
        zin, zout = s * (depth / 2 - 0.045), s * depth / 2
        a = it.ring(at(0, 0, zin), radius + 0.012, n)
        b = it.ring(at(0, 0, zout), radius + 0.012, n)
        c = it.ring(at(0, 0, zout), radius - 0.005, n)
        if s < 0:
            it.bridge(b, a, "hoop")
            it.bridge(c, b, "hoop")
        else:
            it.bridge(a, b, "hoop")
            it.bridge(b, c, "hoop")
        inner = shell[0] if s < 0 else shell[1]
        it.bridge(inner, a, "hoop") if s < 0 else it.bridge(a, inner, "hoop")
        it.cap(c, "head", flip=s < 0)
    # Cords: a zigzag of 8 V's round the shell, just outside it.
    k = 8
    for i in range(2 * k):
        a0 = 2 * math.pi * i / (2 * k)
        a1 = 2 * math.pi * (i + 1) / (2 * k)
        z0 = -depth / 2 + 0.05 if i % 2 == 0 else depth / 2 - 0.05
        z1 = depth / 2 - 0.05 if i % 2 == 0 else -depth / 2 + 0.05
        r = radius + 0.006
        p0 = at(math.cos(a0) * r, math.sin(a0) * r, z0)
        p1 = at(math.cos(a1) * r, math.sin(a1) * r, z1)
        am = (a0 + a1) / 2
        it.strip([p0, p1], 0.008, "cord", normal=(math.cos(am), math.sin(am), 0.0))
    it.sockets["socket.head"] = at(0, 0, depth / 2)
    return it


def drumstick(length=0.42):
    """A turned drumstick: thick butt, tapered shaft, a bead at the end (`socket.tip`).
    Origin at the grip (a third up from the butt)."""
    it = Item("drumstick")
    it.keys = {"stick": "wood"}
    z0 = -length / 3
    it.tube([((0, 0, z0), 0.011), ((0, 0, z0 + length * 0.4), 0.010), ((0, 0, z0 + length - 0.03), 0.006),
             ((0, 0, z0 + length - 0.02), 0.009), ((0, 0, z0 + length), 0.0)], "stick", n=6)
    it.sockets["socket.tip"] = (0.0, 0.0, z0 + length)
    return it


def colour(staff=2.9, cloth=(1.8, 1.65), grip=0.9, cols=8, rows=6):
    """A regimental colour on its pike: an ash staff (`grip` m above the butt in the hand),
    a gilt spear-point finial (`socket.tip`) and the silk cloth nailed to the staff below
    it, flying to +X. The cloth is a `cols` x `rows` grid with groups `cloth` (all its
    vertices) and `pin` (the hoist on the staff): the runtime cloth later, a flat sheet
    now. `socket.top` is the staff's head under the finial."""
    it = Item("colour")
    it.keys = {"staff": "wood", "finial": "brass", "cloth": "colour"}
    z0, z1 = -grip, staff - grip
    it.tube([((0, 0, z0), 0.016), ((0, 0, z1), 0.014)], "staff", n=6)
    it.tube([((0, 0, z1), 0.018), ((0, 0, z1 + 0.04), 0.022), ((0, 0, z1 + 0.20), 0.0)], "finial", n=4)
    w, h = cloth
    top = z1 - 0.04
    grid = []
    for r in range(rows + 1):
        row = []
        for c in range(cols + 1):
            x = 0.02 + w * c / cols
            # A gentle wave so it doesn't read as a board before the runtime cloth.
            y = 0.04 * math.sin(math.pi * 1.5 * c / cols) * (c / cols)
            row.append(it.v((x, y, top - h * r / rows)))
        grid.append(row)
    for r in range(rows):
        for c in range(cols):
            it.f((grid[r][c], grid[r + 1][c], grid[r + 1][c + 1], grid[r][c + 1]), "cloth")
    it.groups["cloth"] = [i for row in grid for i in row]
    it.groups["pin"] = [row[0] for row in grid]
    it.sockets["socket.tip"] = (0.0, 0.0, z1 + 0.20)
    it.sockets["socket.top"] = (0.0, 0.0, z1)
    return it


def _hilt(it, grip=0.10, guard="shell", knuckle=True):
    """Grip, pommel and guard round the origin (the hand); the blade starts at z = grip/2."""
    g = grip / 2
    it.tube([((0, 0, -g), 0.013), ((0, 0, g), 0.012)], "grip", n=6)
    it.tube([((0, 0, -g - 0.035), 0.0), ((0, 0, -g - 0.03), 0.014), ((0, 0, -g - 0.005), 0.016),
             ((0, 0, -g), 0.0)], "hilt", n=6, caps=False)
    if guard == "shell":
        ring = it.ring((0, 0, g + 0.005), 0.045, 8, scale=(0.6, 1.0))
        # A shallow lens: dished up to `c` above, down to `c2` below (two faces on the same
        # three vertices would be no valid mesh).
        c, c2 = it.v((0, 0, g + 0.012)), it.v((0, 0, g + 0.001))
        for i in range(8):
            it.f((ring[i], ring[(i + 1) % 8], c), "hilt")
            it.f((ring[(i + 1) % 8], ring[i], c2), "hilt")
    else:
        it.box((0, 0, g + 0.006), (0.016, 0.10, 0.012), "hilt")
    if knuckle:
        pts = [(0, -0.012 - 0.04 * math.sin(math.pi * t), -g - 0.02 + (2 * g + 0.03) * t) for t in (0, .25, .5, .75, 1)]
        it.strip(pts, 0.008, "hilt")
    return g + 0.012


def smallsword(blade=0.80):
    """A smallsword: slim triangular-section blade (`socket.tip`), a shell guard, knuckle
    bow and pommel (`metal` blade, `brass` hilt, `wood` grip)."""
    it = Item("smallsword")
    it.keys = {"blade": "metal", "hilt": "brass", "grip": "wood"}
    z0 = _hilt(it, 0.09, "shell")
    it.blade(z0, blade, 0.022, 0.010, "blade", points=4)
    it.sockets["socket.tip"] = (0.0, -0.022 * 0.15, z0 + blade + 0.022 * 0.25)
    return it


def hanger(blade=0.64):
    """The private's hanger (1751 pattern): a short, slightly curved single-edged blade,
    a cast brass hilt with a knuckle bow (`socket.tip` at the point)."""
    it = Item("hanger")
    it.keys = {"blade": "metal", "hilt": "brass", "grip": "brass"}
    z0 = _hilt(it, 0.10, "bar")
    it.blade(z0, blade, 0.032, 0.007, "blade", points=4, curve=0.05, edge="one")
    it.sockets["socket.tip"] = (0.0, -0.032 * 0.15 + 0.05, z0 + blade + 0.032 * 0.25)
    return it


def officer_sword(blade=0.82):
    """An officer's sword (spadroon): a straight, broader single-edged blade, a brass
    hilt with a knuckle bow and a shell (`socket.tip`)."""
    it = Item("officer_sword")
    it.keys = {"blade": "metal", "hilt": "brass", "grip": "wood"}
    z0 = _hilt(it, 0.10, "shell")
    it.blade(z0, blade, 0.028, 0.008, "blade", points=4, edge="one")
    it.sockets["socket.tip"] = (0.0, -0.028 * 0.15, z0 + blade + 0.028 * 0.25)
    return it


def _shaft(it, length, grip, r=0.016):
    it.tube([((0, 0, -grip), r), ((0, 0, length - grip), r * 0.9)], "shaft", n=6)
    return length - grip


def spontoon(length=2.1, grip=0.9):
    """An officer's spontoon: an ash shaft, a leaf-shaped steel head with a crossbar and
    socket (`socket.tip`)."""
    it = Item("spontoon")
    it.keys = {"shaft": "wood", "head": "metal"}
    top = _shaft(it, length, grip)
    it.tube([((0, 0, top - 0.10), 0.018), ((0, 0, top + 0.02), 0.014)], "head", n=6)
    it.box((0, 0, top + 0.03), (0.016, 0.16, 0.016), "head")
    it.blade(top + 0.04, 0.26, 0.07, 0.012, "head", points=3)
    it.sockets["socket.tip"] = (0.0, -0.07 * 0.15, top + 0.04 + 0.26 + 0.07 * 0.25)
    return it


def halberd(length=2.0, grip=0.85):
    """A sergeant's halberd: shaft, top spike (`socket.tip`), an axe blade forward (-Y)
    and a back fluke (+Y), with langets down the shaft."""
    it = Item("halberd")
    it.keys = {"shaft": "wood", "head": "metal"}
    top = _shaft(it, length, grip)
    it.box((0, 0, top - 0.12), (0.024, 0.024, 0.30), "head")
    it.blade(top + 0.02, 0.28, 0.04, 0.012, "head", points=3)
    # Axe: a flat plate, the edge forward, wider at the edge.
    it.box((0, -0.10, top - 0.06), (0.008, 0.16, 0.10), "head", taper=(1.0, 1.0))
    a = it.box((0, -0.19, top - 0.06), (0.006, 0.02, 0.20), "head")
    del a
    # Fluke: a curved back spike.
    it.tube([((0, 0.015, top - 0.04), 0.012), ((0, 0.08, top - 0.07), 0.008), ((0, 0.14, top - 0.12), 0.0)],
            "head", n=4)
    it.sockets["socket.tip"] = (0.0, -0.04 * 0.15, top + 0.02 + 0.28 + 0.04 * 0.25)
    return it


def spear(length=2.0, grip=0.8, head=0.22):
    """A spear: wooden shaft, a leaf-shaped head (iron or stone, `metal` by default; a
    family remaps it to `stone`), bound to the shaft (`socket.tip`)."""
    it = Item("spear")
    it.keys = {"shaft": "wood", "head": "metal", "binding": "cord"}
    top = _shaft(it, length, grip, r=0.015)
    it.tube([((0, 0, top - 0.08), 0.019), ((0, 0, top + 0.01), 0.017)], "binding", n=6)
    it.blade(top, head, 0.05, 0.014, "head", points=3)
    it.sockets["socket.tip"] = (0.0, -0.05 * 0.15, top + head + 0.05 * 0.25)
    return it


def longbow(length=1.45, brace=0.15):
    """A Woodlands self bow, braced: limbs of tapering section curving back (+Y) to the
    nocks, a bound grip at the origin and the string (`cord`) behind. `socket.nock` the
    middle of the string (where the drawing hand takes it)."""
    it = Item("longbow")
    it.keys = {"bow": "wood", "grip": "cord", "string": "cord"}
    half = length / 2
    k = 6
    for s in (-1, 1):
        path = []
        for i in range(k + 1):
            t = i / k
            z = s * half * t * (1.0 - 0.04 * t)
            y = (brace - 0.02) * t * t * 0.55
            path.append(((0, y, z), 0.016 * (1.0 - 0.6 * t) + 0.004))
        it.tube(path, "bow", n=5, scale=(0.75, 1.0))
    it.tube([((0, 0, -0.06), 0.019), ((0, 0, 0.06), 0.019)], "grip", n=5)
    ny = (brace - 0.02) * 0.55
    nz = half * 0.96
    it.strip([(0, ny, -nz), (0, brace * 0.7, 0), (0, ny, nz)], 0.003, "string", normal=(0, 1, 0))
    it.sockets["socket.nock"] = (0.0, brace * 0.7, 0.0)
    return it


def vane(it, pts, part, gap=0.0004):
    """A two-sided triangle (a fletching): two faces on their own vertices, `gap` m apart
    along the normal (one face twice over the same vertices is no valid mesh)."""
    a, b, c = pts
    n = _norm(_cross(_sub(b, a), _sub(c, a)))
    front = [it.v(tuple(p[i] + n[i] * gap / 2 for i in range(3))) for p in pts]
    back = [it.v(tuple(p[i] - n[i] * gap / 2 for i in range(3))) for p in pts]
    it.f(front, part)
    it.f(tuple(reversed(back)), part)


def arrow(length=0.72):
    """An arrow: cane or wood shaft, a stone or iron head (`socket.tip`) and three
    fletchings. Origin at the nock (the drawing hand)."""
    it = Item("arrow")
    it.keys = {"shaft": "wood", "head": "stone", "fletch": "feather"}
    it.tube([((0, 0, 0), 0.0045), ((0, 0, length - 0.05), 0.0045)], "shaft", n=4)
    it.blade(length - 0.05, 0.05, 0.018, 0.004, "head", points=2)
    for i in range(3):
        a = 2 * math.pi * i / 3
        d = (math.cos(a), math.sin(a), 0.0)
        vane(it, [(d[0] * 0.004, d[1] * 0.004, 0.02), (d[0] * 0.004, d[1] * 0.004, 0.16),
                  (d[0] * 0.016, d[1] * 0.016, 0.05)], "fletch")
    it.sockets["socket.tip"] = (0.0, -0.018 * 0.15, length + 0.018 * 0.25)
    return it


def quiver(arrows=6, length=0.62):
    """A hide quiver with arrow shafts and fletchings showing at the mouth. Origin at the
    strap on its back (+Y), the mouth up; `socket.mouth` the centre of the mouth."""
    it = Item("quiver")
    it.keys = {"quiver": "hide", "shaft": "wood", "fletch": "feather"}
    r = 0.055
    off = (0.0, r, length * 0.6)

    def at(x, y, z):
        return (x - off[0], y - off[1], z - off[2])

    # One closed hide body (bottom and a cover at the mouth the shafts stand in), so the
    # volume check (`gear_volume`) can test it.
    it.tube([(at(0, 0, 0), r * 0.85), (at(0, 0, 0.05), r), (at(0, 0, length), r * 1.05)], "quiver", n=8)
    for i in range(arrows):
        a = 2 * math.pi * i / arrows
        x, y = math.cos(a) * r * 0.5, math.sin(a) * r * 0.5
        it.tube([(at(x, y, length - 0.25), 0.004), (at(x, y, length + 0.12), 0.004)], "shaft", n=3)
        vane(it, [at(x, y, length + 0.03), at(x, y, length + 0.12),
                  at(x + 0.012 * math.cos(a), y + 0.012 * math.sin(a), length + 0.05)], "fletch")
    it.sockets["socket.mouth"] = at(0, 0, length)
    return it


def war_club(length=0.62):
    """A Woodlands ball-headed club: one piece of hardwood, the handle curving forward into
    a ball held in the 'jaws' of the head, a short spike set in the ball (`socket.tip` on
    the ball's striking face). Origin at the grip near the handle's end."""
    it = Item("war_club")
    it.keys = {"club": "wood", "spike": "metal"}
    path = []
    for i in range(7):
        t = i / 6
        z = -0.10 + t * (length - 0.16)
        y = -0.10 * t * t
        path.append(((0, y, z), 0.016 + 0.01 * t))
    it.tube(path, "club", n=6)
    c = (0.0, -0.115, length - 0.26 + 0.03)  # the ball sits on the handle's end
    rb = 0.05
    rings = [[it.v((c[0], c[1], c[2] - rb))]]
    for k in range(1, 5):
        a = math.pi * k / 5
        rings.append(it.ring((c[0], c[1], c[2] - rb * math.cos(a)), rb * math.sin(a), 8))
    rings.append([it.v((c[0], c[1], c[2] + rb))])
    for a, b in zip(rings, rings[1:]):
        if len(a) == 1:
            for i in range(8):
                it.f((a[0], b[(i + 1) % 8], b[i]), "club")
        elif len(b) == 1:
            for i in range(8):
                it.f((a[i], a[(i + 1) % 8], b[0]), "club")
        else:
            it.bridge(a, b, "club")
    it.tube([((0, c[1] - rb + 0.01, c[2]), 0.006), ((0, c[1] - rb - 0.04, c[2]), 0.0)], "spike", n=4)
    it.sockets["socket.tip"] = (0.0, c[1] - rb, c[2])
    return it


def tomahawk(length=0.50):
    """A trade tomahawk: a wooden haft and an iron head with a flared edge forward (-Y) and
    a short poll (`socket.tip` at the middle of the edge). Origin at the grip."""
    it = Item("tomahawk")
    it.keys = {"haft": "wood", "head": "metal"}
    z1 = length - 0.12
    it.tube([((0, 0, -0.12), 0.013), ((0, 0, z1 + 0.03), 0.012)], "haft", n=6)
    it.box((0, 0, z1), (0.03, 0.035, 0.05), "head")
    it.box((0, -0.075, z1), (0.012, 0.12, 0.045), "head", taper=(0.5, 1.0))
    it.box((0, -0.14, z1), (0.006, 0.012, 0.10), "head")
    it.sockets["socket.tip"] = (0.0, -0.146, z1)
    return it


def macuahuitl(length=0.90, blades=7):
    """A macuahuitl: a flat wooden paddle, a short round handle with a wrist hole knob,
    obsidian blades set in grooves along both edges (`stone`), `socket.tip` at the end."""
    it = Item("macuahuitl")
    it.keys = {"club": "wood", "blades": "stone"}
    it.tube([((0, 0, -0.10), 0.016), ((0, 0, 0.10), 0.016)], "club", n=6)
    it.tube([((0, 0, -0.13), 0.0), ((0, 0, -0.12), 0.022), ((0, 0, -0.10), 0.018)], "club", n=6, caps=False)
    w, top = 0.085, length - 0.10
    it.box((0, 0, (0.10 + top) / 2), (0.022, w, top - 0.10), "club", taper=(0.9, 1.15))
    for s in (-1, 1):
        for i in range(blades):
            z = 0.16 + (top - 0.20) * (i + 0.5) / blades
            ww = w * (1.0 + 0.15 * (z - 0.10) / (top - 0.10)) / 2
            vane(it, [(0.0, s * ww, z - 0.035), (0.0, s * ww, z + 0.035), (0.0, s * (ww + 0.025), z + 0.005)],
                 "blades", gap=0.002)
    it.sockets["socket.tip"] = (0.0, 0.0, top)
    return it


def sling(cord=0.55):
    """An Andean sling hanging from the finger loop (origin): two braided cords (`cord`)
    down to the woven pouch; `socket.pouch` (the stone) and `socket.release` (the free end,
    held with the loop until the cast)."""
    it = Item("sling")
    it.keys = {"cord": "cord", "pouch": "cord"}
    for s in (-1, 1):
        pts = [(s * 0.01 * t, 0.0, -cord * t) for t in (0.0, 0.33, 0.66, 1.0)]
        pts[-1] = (s * 0.03, 0.0, -cord)
        it.strip(pts, 0.006, "cord", normal=(0, 1, 0))
    # The pouch: a lens of cloth round the stone.
    c = (0.0, 0.0, -cord - 0.025)
    ring = it.ring(c, 0.035, 8, frame=((1.0, 0.0, 0.0), (0.0, 0.0, 1.0)), scale=(1.0, 0.7))
    for sy in (-1, 1):
        p = it.v((c[0], c[1] + sy * 0.018, c[2]))
        for i in range(8):
            a, b = ring[i], ring[(i + 1) % 8]
            it.f((a, b, p) if sy < 0 else (b, a, p), "pouch")
    it.sockets["socket.pouch"] = c
    it.sockets["socket.release"] = (0.03, 0.0, 0.0)
    return it


def shield(radius=0.30):
    """A round hide shield, slightly domed, with a wrapped rim; its face looks -Y. Origin
    at the hand grip behind the centre; `socket.arm` the forearm strap."""
    it = Item("shield")
    it.keys = {"face": "hide", "rim": "wood"}
    n = 16
    dome = 0.05
    centre = it.v((0.0, -0.06 - dome, 0.0))
    frame = ((1.0, 0.0, 0.0), (0.0, 0.0, 1.0))
    r1 = it.ring((0.0, -0.06 - dome * 0.7, 0.0), radius * 0.5, n, frame)
    r2 = it.ring((0.0, -0.06, 0.0), radius, n, frame)
    r3 = it.ring((0.0, -0.04, 0.0), radius * 1.02, n, frame)
    r4 = it.ring((0.0, -0.035, 0.0), radius * 0.95, n, frame)
    for i in range(n):
        it.f((r1[i], r1[(i + 1) % n], centre), "face")
    it.bridge(r2, r1, "face")
    it.bridge(r3, r2, "rim")
    it.bridge(r4, r3, "rim")
    it.cap(r4, "face", flip=True)
    it.box((0.0, -0.02, 0.0), (0.03, 0.03, 0.12), "rim")
    it.sockets["socket.arm"] = (0.0, -0.02, 0.12)
    return it


def pistol(length=0.48):
    """A flintlock pistol (sea-service style): a round barrel along +Z to the muzzle
    (`socket.muzzle`), the stock and the bird's-head butt down and back from the grip
    (origin), a lock plate and cock, brass butt cap."""
    it = Item("pistol")
    it.keys = {"stock": "wood", "barrel": "metal", "cap": "brass"}
    bz0, bz1 = 0.05, length - 0.10
    it.tube([((0, -0.02, bz0), 0.011), ((0, -0.02, bz1), 0.0095)], "barrel", n=6)
    it.box((0, -0.005, (bz0 + bz1) / 2 - 0.03), (0.026, 0.024, bz1 - bz0 - 0.06), "stock")
    it.box((0, 0.0, 0.0), (0.028, 0.04, 0.10), "stock", shift=(0.0, -0.01))
    it.box((0, 0.025, -0.08), (0.03, 0.045, 0.08), "stock", shift=(0.0, -0.03))
    it.box((0, 0.035, -0.125), (0.034, 0.05, 0.02), "cap")
    it.box((0.015, -0.015, 0.06), (0.004, 0.03, 0.06), "barrel")
    it.box((0.018, -0.03, 0.09), (0.006, 0.01, 0.025), "barrel")
    it.sockets["socket.muzzle"] = (0.0, -0.02, bz1)
    return it


def blowpipe(length=1.80):
    """An Andean blowpipe (`IncTrb`, `VAFLIA`): a cane tube bound with cord at the mouth end
    and at the middle. Origin at the right hand's grip just before the mouthpiece
    (`socket.mouth`, 0.04 m behind), the bore along +Z to `socket.muzzle`."""
    it = Item("blowpipe")
    it.keys = {"pipe": "wood", "binding": "cord"}
    it.tube([((0, 0, -0.04), 0.016), ((0, 0, length - 0.04), 0.012)], "pipe", n=6)
    for z in (0.02, length * 0.5):
        it.tube([((0, 0, z - 0.03), 0.018), ((0, 0, z + 0.03), 0.018)], "binding", n=6)
    it.sockets["socket.mouth"] = (0.0, 0.0, -0.04)
    it.sockets["socket.muzzle"] = (0.0, 0.0, length - 0.04)
    return it


def matchlock(length=1.45):
    """A 17th-century matchlock musket in `line`'s musket frame (gear.musket: +Z along the
    barrel, origin at the grip 0.25 m above the butt, the barrel 1 cm in front of the
    axis): a heavy straight stock with a fishtail butt, a long round barrel, the lock plate
    on the right with the pan and the serpentine (the S-shaped arm holding the match over
    the pan), the trigger bar below, and a short length of lit match in the serpentine's
    jaws. `socket.muzzle` at the muzzle."""
    it = Item("matchlock")
    it.keys = {"stock": "wood", "barrel": "metal", "match": "cord"}
    o = -0.25  # the butt
    # Fishtail butt, wrist, fore-stock to 0.2 m short of the muzzle.
    it.box((0, 0.012, o + 0.14), (0.045, 0.12, 0.28), "stock", taper=(0.75, 0.5), shift=(0.0, -0.02))
    it.box((0, 0.0, o + 0.36), (0.034, 0.05, 0.16), "stock")
    fz0, fz1 = o + 0.44, o + length - 0.20
    it.box((0, 0.0, (fz0 + fz1) / 2), (0.03, 0.036, fz1 - fz0), "stock", taper=(0.85, 0.8))
    # Barrel: octagonal at the breech, round on.
    it.tube([((0, -0.01, o + 0.40), 0.0125), ((0, -0.01, o + 0.62), 0.012), ((0, -0.01, o + length), 0.0105)],
            "barrel", n=6)
    # Lock plate, pan and the serpentine on the right (-X).
    it.box((-0.018, 0.002, o + 0.43), (0.004, 0.026, 0.13), "barrel")
    it.box((-0.020, -0.012, o + 0.47), (0.016, 0.010, 0.022), "barrel")  # pan
    s0 = (-0.022, 0.020, o + 0.39)
    it.tube([(s0, 0.004), ((-0.024, 0.030, o + 0.42), 0.004), ((-0.024, 0.008, o + 0.45), 0.0035),
             ((-0.023, -0.016, o + 0.462), 0.0035)], "barrel", n=4)
    # Trigger bar: a long lever under the stock.
    it.box((0, 0.034, o + 0.36), (0.006, 0.010, 0.10), "barrel")
    # The match in the jaws, its lit end over the pan.
    it.tube([((-0.023, -0.016, o + 0.462), 0.003), ((-0.023, -0.03, o + 0.50), 0.003)], "match", n=4)
    it.sockets["socket.muzzle"] = (0.0, -0.01, o + length)
    return it


def musket_rest(length=1.25, grip=0.95):
    """The forked musket rest: an ash staff with an iron fork (a U to lay the barrel in)
    on top, an iron spike at the foot. Origin at the left hand's grip (`grip` m above the
    foot), +Z up the staff; `socket.fork` in the fork."""
    it = Item("musket_rest")
    it.keys = {"staff": "wood", "iron": "metal"}
    z0, z1 = -grip, length - grip
    it.tube([((0, 0, z0 + 0.06), 0.012), ((0, 0, z1 - 0.06), 0.011)], "staff", n=5)
    it.tube([((0, 0, z0), 0.0), ((0, 0, z0 + 0.07), 0.008)], "iron", n=4, caps=False)
    fork = [((-0.05, 0, z1 + 0.04), 0.004), ((-0.045, 0, z1 - 0.01), 0.005), ((-0.02, 0, z1 - 0.05), 0.006),
            ((0, 0, z1 - 0.06), 0.007), ((0.02, 0, z1 - 0.05), 0.006), ((0.045, 0, z1 - 0.01), 0.005),
            ((0.05, 0, z1 + 0.04), 0.004)]
    it.tube(fork, "iron", n=4)
    it.sockets["socket.fork"] = (0.0, 0.0, z1 - 0.03)
    return it


ITEMS = {
    "drum": drum, "drumstick": drumstick, "colour": colour, "smallsword": smallsword, "hanger": hanger,
    "officer_sword": officer_sword, "spontoon": spontoon, "halberd": halberd, "longbow": longbow, "arrow": arrow,
    "quiver": quiver, "spear": spear, "war_club": war_club, "tomahawk": tomahawk, "macuahuitl": macuahuitl,
    "sling": sling, "shield": shield, "pistol": pistol, "blowpipe": blowpipe,
    "matchlock": matchlock, "musket_rest": musket_rest,
}
# Triangle budget per item (a soldier is ~12k triangles; held gear stays small).
BUDGET = {
    "drum": 400, "drumstick": 50, "colour": 200, "smallsword": 150, "hanger": 120, "officer_sword": 150,
    "spontoon": 120, "halberd": 150, "longbow": 160, "arrow": 40, "quiver": 220, "spear": 100, "war_club": 160,
    "tomahawk": 80, "macuahuitl": 140, "sling": 60, "shield": 180, "pistol": 100, "blowpipe": 80,
    "matchlock": 160, "musket_rest": 80,
}
# The point a strike reaches with (docs/melee.md): melee weapons carry `socket.tip`,
# firearms `socket.muzzle`; the rest have their own sockets.
TIP = {"drumstick", "colour", "smallsword", "hanger", "officer_sword", "spontoon", "halberd", "arrow", "spear",
       "war_club", "tomahawk", "macuahuitl"}
MUZZLE = {"pistol", "blowpipe", "matchlock"}
SOCKETS = {"drum": {"socket.head"}, "colour": {"socket.top"}, "longbow": {"socket.nock"}, "quiver": {"socket.mouth"},
           "sling": {"socket.pouch", "socket.release"}, "shield": {"socket.arm"}, "musket_rest": {"socket.fork"}}


def build(name, **kw):
    """The `Item` of library item `name` (keyword arguments to its builder)."""
    return ITEMS[name](**kw)


# Grips (#287 m2-grips): where the hands close on each item, in the item frame (origin at
# the main grip, +Z the long axis). `main` the holding hand's grip, `off` the other hand's
# on a two-handed item (None: one-handed, the off hand stays a loose fist): (z along +Z,
# diameter in metres). The grip axis is the item's +Z through (0, 0, z); the fist closes
# round it (`grips.py`), its thumb towards +Z.
class Grip:
    __slots__ = ("main", "off")

    def __init__(self, main, off=None):
        self.main, self.off = main, off

    def point(self, which="main"):
        g = getattr(self, which)
        return None if g is None else (0.0, 0.0, g[0])


GRIPS = {
    # The musket (`gear.musket`, `matchlock`): the wrist of the stock, the fore-stock under the
    # barrel where `aim.hold` puts the left hand (0.34 m on).
    "musket": Grip((0.0, 0.036), (0.34, 0.040)),
    "matchlock": Grip((0.0, 0.036), (0.34, 0.040)),
    # Pole arms: two points along the shaft (the clips' `left_along`).
    "pike": Grip((0.0, 0.032), (0.45, 0.032)),
    "spear": Grip((0.0, 0.030), (0.45, 0.030)),
    "spontoon": Grip((0.0, 0.032), (0.45, 0.032)),
    "halberd": Grip((0.0, 0.032), (0.45, 0.032)),
    "colour": Grip((0.0, 0.032), (0.32, 0.032)),
    "musket_rest": Grip((0.0, 0.030)),
    # Hilts and hafts.
    "smallsword": Grip((0.0, 0.025)),
    "hanger": Grip((0.0, 0.025)),
    "officer_sword": Grip((0.0, 0.025)),
    "war_club": Grip((0.0, 0.034)),
    "tomahawk": Grip((0.0, 0.026)),
    "macuahuitl": Grip((0.0, 0.032)),
    "pistol": Grip((0.0, 0.030)),
    "blowpipe": Grip((0.0, 0.026), (0.42, 0.026)),
    # The bow's riser, the drumstick's butt third.
    "longbow": Grip((0.0, 0.038)),
    "drumstick": Grip((0.0, 0.021)),
    # Pinched or hooked rather than gripped: the arrow's nock, the sling's loop, the shield's
    # strap; their fists close loosely (the default diameter).
    "arrow": Grip((0.0, 0.009)),
    "sling": Grip((0.0, 0.010)),
    "shield": Grip((0.0, 0.030)),
}
# The fist a hand closes to with no grip spec (or an empty hand): the diameter it rolls round.
FIST = 0.028


def grip(name):
    """The `Grip` of item `name` (None for an item held otherwise, e.g. the drum on a hook)."""
    return GRIPS.get(name)

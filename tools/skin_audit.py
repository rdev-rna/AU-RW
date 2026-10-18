"""Audit diagnostic JSON, not game files. ACU-RE-0015; no skinning claim.

Inputs: acu-mesh --skin reports and the independent sequential native-LZO
skeleton probe. No axis conversion, guessed remapping or missing-bone fallback.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path


def multiply(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(4))
             for j in range(4)] for i in range(4)]


def global_matrix(bone):
    q = bone['global_rotation']
    p = bone['global_position']
    if len(q) != 4 or len(p) != 4 or not all(math.isfinite(v) for v in q + p):
        raise ValueError('invalid skeleton transform')
    norm = math.sqrt(sum(v * v for v in q))
    if abs(norm - 1) > 1e-4:
        raise ValueError('nonunit skeleton quaternion')
    x, y, z, w = [v / norm for v in q]
    return [[1-2*(y*y+z*z), 2*(x*y-z*w), 2*(x*z+y*w), p[0]],
            [2*(x*y+z*w), 1-2*(x*x+z*z), 2*(y*z-x*w), p[1]],
            [2*(x*z-y*w), 2*(y*z+x*w), 1-2*(x*x+y*y), p[2]],
            [0, 0, 0, 1]]


def unpack_matrix(values, column_major):
    if len(values) != 16 or not all(math.isfinite(v) for v in values):
        raise ValueError('invalid mesh matrix')
    return [[values[j*4+i if column_major else i*4+j]
             for j in range(4)] for i in range(4)]


def audit(mesh, skeleton):
    if mesh.get('typed_status') != 'parsed' or not skeleton.get('parsed'):
        raise ValueError('unparsed input')
    bones = mesh['mesh_bones']
    lookup = {}
    for bone in skeleton['bones']:
        name = bone['name']
        if name in lookup:
            raise ValueError('ambiguous skeleton name hash')
        lookup[name] = bone
    parts, records, vertices = (mesh[k] for k in
                               ('skin_parts', 'skin_instancing', 'skin_vertices'))
    if len(parts) != len(records):
        raise ValueError('part/instancing count mismatch')
    active = set()
    weight_sums = set()
    covered = set()
    for part, record in zip(parts, records):
        start, count = part['vertex_base'], part['vertex_count']
        if start < 0 or count < 0 or start + count > len(vertices):
            raise ValueError('part vertex span outside buffer')
        if count != record['vertex_count']:
            raise ValueError('part/instancing vertex count mismatch')
        indices = set(range(start, start + count))
        if covered & indices:
            raise ValueError('overlapping part spans')
        covered.update(indices)
        palette = record['hardware_to_mesh_bones']
        if any(i < 0 or i >= len(bones) for i in palette):
            raise ValueError('palette outside mesh bone table')
        for v in vertices[start:start + count]:
            joints, weights = v['joints'], v['weights']
            if joints is None or weights is None or len(joints) != len(weights):
                raise ValueError('missing or unequal joint/weight slots')
            if len(joints) not in (4, 8):
                raise ValueError('unsupported joint slot count')
            if any(type(x) is not int or not 0 <= x <= 255 for x in joints + weights):
                raise ValueError('joint/weight is not a byte')
            weight_sums.add(sum(weights))
            if sum(weights) == 0:
                raise ValueError('zero total weight')
            for j, weight in zip(joints, weights):
                if weight:
                    if j >= len(palette):
                        raise ValueError('weighted joint outside hardware palette')
                    active.add(palette[j])
    if len(covered) != len(vertices):
        raise ValueError('vertices outside part spans')

    matched, unresolved = [], []
    for i in sorted(active):
        mb = bones[i]
        sk = lookup.get(int(mb['name_hash'], 16))
        if sk is None:
            unresolved.append({'mesh_bone': i, 'name_hash': mb['name_hash']})
        else:
            matched.append((i, mb, sk))
    candidates = []
    for column_major in (True, False):
        for skeleton_first in (True, False):
            products = []
            for i, mb, sk in matched:
                s = global_matrix(sk)
                b = unpack_matrix(mb['matrix'], column_major)
                products.append(multiply(s, b) if skeleton_first else multiply(b, s))
            if products:
                mean = [[sum(p[i][j] for p in products)/len(products)
                         for j in range(4)] for i in range(4)]
                spread = max(abs(p[i][j] - mean[i][j]) for p in products
                             for i in range(4) for j in range(4))
            else:
                mean, spread = None, None
            candidates.append({'storage': 'column_major' if column_major else 'row_major',
                               'product': 'S*B' if skeleton_first else 'B*S',
                               'mean': mean, 'max_deviation_from_mean': spread})
    return {'name': mesh['name'], 'file_id': mesh['file_id'],
            'payload_sha256': mesh['payload_sha256'],
            'payload_length': mesh['payload_length'], 'payload_range': mesh['payload_range'],
            'vertices': len(vertices), 'parts': len(parts), 'weight_sums': sorted(weight_sums),
            'mesh_bones': len(bones), 'active_bones': len(active),
            'matches': [{'mesh_bone': i, 'name_hash': mb['name_hash'],
                         'skeleton_bone': sk['index'], 'skeleton_bone_id': sk['id']}
                        for i, mb, sk in matched],
            'unresolved': unresolved, 'matrix_candidates': candidates}


def identified_json(path):
    raw = path.read_bytes()
    return json.loads(raw), {'path': str(path.resolve()), 'sha256': hashlib.sha256(raw).hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--skeleton-probe', type=Path, required=True)
    parser.add_argument('--skeleton', default='ACU_BaseSkeleton_Male')
    parser.add_argument('--mesh-reports', type=Path, nargs='+', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    rigs, source = identified_json(args.skeleton_probe)
    selected = [r for r in rigs if r['name'] == args.skeleton]
    if len(selected) != 1:
        raise ValueError('skeleton selection missing or ambiguous')
    rig = selected[0]
    report = {'finding': 'ACU-RE-0015', 'skeleton_input': source,
              'skeleton': {k: rig[k] for k in ('name', 'file_id', 'payload_sha256')},
              'method': 'Hamilton xyzw unit-normalized quaternion; xyz position; four matrix candidates',
              'limits': 'Diagnostic name-hash matches only; no native axes/units, additive-rig selection or skinning proof',
              'mesh_inputs': [], 'results': []}
    for path in args.mesh_reports:
        data, source = identified_json(path)
        source.update({k: data[k] for k in ('archive', 'entry', 'entry_record',
                      'stored_sha256', 'decoded_length', 'decoded_sha256')})
        report['mesh_inputs'].append(source)
        for mesh in data['resources']:
            result = audit(mesh, rig)
            result['mesh_input'] = len(report['mesh_inputs']) - 1
            report['results'].append(result)
    args.output.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    for r in report['results']:
        print(f"{r['name']}: {len(r['matches'])}/{r['active_bones']} active bones matched; "
              f"{r['vertices']} vertices; sums={r['weight_sums']}")


if __name__ == '__main__':
    main()

"""Independent analytical vectors and malformed palette cases for ACU-RE-0015."""
import copy
import math
import unittest

from skin_audit import audit, global_matrix, multiply


def fixture():
    # S0=identity; S1=translate(1,2,3). B_i = inverse(S_i)*C,
    # where C rotates +90 degrees around Z and translates (4,5,6).
    matrices = [[0, 1, 0, 0, -1, 0, 0, 0, 0, 0, 1, 0, 4, 5, 6, 1],
                [0, 1, 0, 0, -1, 0, 0, 0, 0, 0, 1, 0, 3, 3, 3, 1]]
    mesh = {'typed_status': 'parsed', 'name': 'synthetic', 'file_id': '0000000000000001',
            'payload_sha256': 'fixture', 'payload_length': 0, 'payload_range': {'start': 0, 'end': 0},
            'mesh_bones': [{'name_hash': f'{i:08x}', 'matrix': m} for i, m in enumerate(matrices)],
            'skin_parts': [{'vertex_base': 0, 'vertex_count': 2}],
            'skin_instancing': [{'vertex_count': 2, 'hardware_to_mesh_bones': [1, 0]}],
            'skin_vertices': [{'joints': [0, 1, 255, 255], 'weights': [128, 127, 0, 0]},
                              {'joints': [1, 255, 255, 255], 'weights': [255, 0, 0, 0]}]}
    rig = {'parsed': True, 'bones': [
        {'name': i, 'id': f'{i+10:016x}', 'index': i,
         'global_rotation': [0, 0, 0, 1], 'global_position': p}
        for i, p in enumerate(([0, 0, 0, 0], [1, 2, 3, 0]))]}
    return mesh, rig


class SkinAudit(unittest.TestCase):
    def test_palette_and_matrix_alternatives(self):
        mesh, rig = fixture()
        result = audit(mesh, rig)
        self.assertEqual(result['weight_sums'], [255])
        self.assertEqual([r['skeleton_bone'] for r in result['matches']], [0, 1])
        best = result['matrix_candidates'][0]
        self.assertEqual(best['mean'], [[0, -1, 0, 4], [1, 0, 0, 5],
                                       [0, 0, 1, 6], [0, 0, 0, 1]])
        self.assertEqual(best['max_deviation_from_mean'], 0)
        self.assertTrue(all(r['max_deviation_from_mean'] > 0
                            for r in result['matrix_candidates'][1:]))
        mesh['mesh_bones'][0]['name_hash'] = 'ffffffff'
        result = audit(mesh, rig)
        self.assertEqual(result['unresolved'], [{'mesh_bone': 0, 'name_hash': 'ffffffff'}])
        self.assertEqual(len(result['matches']), 1)

    def test_rotation_and_noncommuting_translation(self):
        r = global_matrix({'global_rotation': [0, 0, math.sqrt(.5), math.sqrt(.5)],
                           'global_position': [1, 2, 3, 0]})
        t = [[1, 0, 0, 2], [0, 1, 0, 0], [0, 0, 1, 0], [0, 0, 0, 1]]
        result = multiply(r, t)
        for actual, expected in zip([result[i][3] for i in range(3)], [1, 4, 3]):
            self.assertAlmostEqual(actual, expected)

    def test_invalid_boundaries(self):
        mesh, rig = fixture()
        mutations = [
            lambda m: m['skin_instancing'].clear(),
            lambda m: m['skin_parts'][0].update(vertex_base=-1),
            lambda m: m['skin_parts'][0].update(vertex_count=3),
            lambda m: m['skin_instancing'][0].update(vertex_count=1),
            lambda m: m['skin_instancing'][0].update(hardware_to_mesh_bones=[2, 0]),
            lambda m: m['skin_vertices'][0].update(joints=[2, 1, 0, 0]),
            lambda m: m['skin_vertices'][0].update(weights=[0, 0, 0, 0]),
            lambda m: m['skin_vertices'][0].update(weights=[256, 0, 0, 0]),
            lambda m: m['skin_vertices'][0].update(weights=[255]),
            lambda m: m['skin_vertices'][0].update(joints=None),
            lambda m: m['skin_vertices'].append(m['skin_vertices'][0]),
            lambda m: m['skin_vertices'][0].update(joints=[0, 1], weights=[128, 127]),
            lambda m: m['mesh_bones'][0].update(matrix=[math.nan]*16),
        ]
        for mutation in mutations:
            bad = copy.deepcopy(mesh)
            mutation(bad)
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                audit(bad, rig)
        bad = copy.deepcopy(mesh)
        bad['skin_parts'].append(bad['skin_parts'][0])
        bad['skin_instancing'].append(bad['skin_instancing'][0])
        with self.assertRaisesRegex(ValueError, 'overlapping'):
            audit(bad, rig)
        bad_rig = copy.deepcopy(rig)
        bad_rig['bones'].append(bad_rig['bones'][0])
        with self.assertRaisesRegex(ValueError, 'ambiguous'):
            audit(mesh, bad_rig)
        rig['bones'][0]['global_rotation'] = [0, 0, 0, 0]
        with self.assertRaisesRegex(ValueError, 'nonunit'):
            audit(mesh, rig)


if __name__ == '__main__':
    unittest.main()

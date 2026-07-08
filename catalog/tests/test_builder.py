"""Unit tests for the AIDL catalog builder. Run: python3 -m unittest -v (from catalog/)."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import bindfetto_catalog as bc  # noqa: E402

FIXTURES = os.path.join(os.path.dirname(os.path.abspath(__file__)), "fixtures")


class BuilderTest(unittest.TestCase):
    def test_sequential_codes_and_multiline(self):
        cat = bc.build_catalog([os.path.join(FIXTURES, "IActivityManager.aidl")])
        self.assertEqual(
            cat["android.app.IActivityManager"],
            {1: "getTasks", 2: "startActivity", 3: "noteWakeupAlarm"},
        )

    def test_explicit_codes(self):
        cat = bc.build_catalog([os.path.join(FIXTURES, "ITricky.aidl")])
        self.assertEqual(cat["com.example.IExplicit"], {5: "alpha", 10: "beta"})

    def test_skips_consts_and_nested_types(self):
        cat = bc.build_catalog([os.path.join(FIXTURES, "ITricky.aidl")])
        # getName=1, setValues=2, echo=3, ping=4 — VERSION/NAME consts and the nested
        # parcelable must not consume transaction codes.
        self.assertEqual(
            cat["com.example.ITricky"],
            {1: "getName", 2: "setValues", 3: "echo", 4: "ping"},
        )

    def test_directory_recursion_merges_all(self):
        cat = bc.build_catalog([FIXTURES])
        self.assertEqual(
            set(cat),
            {
                "android.app.IActivityManager",
                "com.example.ITricky",
                "com.example.IExplicit",
            },
        )

    def test_json_is_sorted_and_string_keyed(self):
        cat = bc.build_catalog([os.path.join(FIXTURES, "IActivityManager.aidl")])
        text = bc.to_json(cat)
        self.assertIn('"1": "getTasks"', text)
        self.assertIn('"7"', bc.to_json({"X": {7: "a", 1: "b"}}))
        # numeric-sorted keys within an interface
        self.assertLess(text.index('"1"'), text.index('"3"'))


if __name__ == "__main__":
    unittest.main()

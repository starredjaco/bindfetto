package com.example;

// Exercises: constants, annotations (incl. brace args), generics with commas,
// a nested parcelable, oneway, and explicit transaction ids on another interface.
@SuppressWarnings(value={"inout"})
interface ITricky {
    const int VERSION = 2;
    const String NAME = "svc";

    @nullable String getName();

    void setValues(in int[] vals, inout Map<String, String> m);

    parcelable Nested {
        int z;
    }

    @utf8InCpp String echo(@nullable String s);

    oneway void ping();
}

interface IExplicit {
    void alpha() = 5;
    void beta() = 10;
}

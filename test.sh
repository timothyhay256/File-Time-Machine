#!/bin/bash
#set -e

rm -r demo
cp -r demo.bak demo
find demo -not -path "./demo/.time/*" -type f -exec md5sum {} \; > checklist.chk
cargo run --release -- -c demo.bak/config.json

rm -r demo/*
cp -r src/* demo/
find demo -not -path "demo/.time/*" -type f -exec md5sum {} \; > checklist-two.chk
cargo run --release -- -c demo.bak/config.json

cp -r gui/* demo/ # Do a test that includes pre-existing files. 
find demo -not -path "demo/.time/*" -type f -exec md5sum {} \; > checklist-three.chk
cargo run --release -- -c demo.bak/config.json

echo "Checking demo..."
cargo run --release -- -c demo.bak/config.json restore --restore-index 1
if ! md5sum -c --quiet checklist.chk
then
    echo "demo failed check!"
    exit 0
fi

echo "Checking src..."
cargo run --release -- -c demo.bak/config.json restore --restore-index 2
if ! md5sum -c --quiet checklist-two.chk
then
    echo "src failed check!"
    exit 0
fi

echo "Checking src+gui..."
cargo run --release -- -c demo.bak/config.json restore --restore-index 3
if ! md5sum -c --quiet checklist-three.chk
then
    echo "src+gui failed check!"
    exit 0
fi


printf "\nAll tests passed!"

rm checklist.chk
rm checklist-two.chk
rm checklist-three.chk
cp -r demo.bak demo

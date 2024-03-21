#!/bin/bash
DIR=input-test
VID=$DIR/expected.rvid
JPG="$DIR/expected-jpgs"
ffplay  "$JPG/%04d.jpg" &


DIR=output-test
VID=$DIR/output-small.rvid
JPG="$DIR/output-small-jpgs"

ffplay  "$JPG/%04d.jpg" &

DIR=input-test
VID=$DIR/input.rvid
JPG="$DIR/input-jpgs"
ffplay  "$JPG/%04d.jpg" &

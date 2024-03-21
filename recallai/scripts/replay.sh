#!/bin/bash

read -p "skip xcode/extract real: " MMM
DIR=output
VID=$DIR/output.rvid
JPG="$DIR/output-jpgs"
W=1536
W=4000
W=1024
if [[ -z $MMM ]] || [[ "$MMM" == "n" ]]; then
	./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json -o $VID
	echo "extracting $JPG"
	./target/release/extract-images -i $VID -o $JPG --frame-min 0 --frame-max 100 --max-width $W &
	A=$!
	./target/release/extract-images -i $VID -o $JPG --frame-min 100 --frame-max 200 --max-width $W &
	B=$!
	./target/release/extract-images -i $VID -o $JPG --frame-min 200 --frame-max 300 --max-width $W &
	C=$!
	./target/release/extract-images -i $VID -o $JPG --frame-min 300 --frame-max 400 --max-width $W &
	D=$!
	./target/release/extract-images -i $VID -o $JPG --frame-min 400 --frame-max 500 --max-width $W &
	E=$!
	./target/release/extract-images -i $VID -o $JPG --frame-min 500 --frame-max 600 --max-width $W &
	F=$!
	./target/release/extract-images -i $VID -o $JPG --frame-min 600 --frame-max 1000 --max-width $W 
	wait $A
	wait $B
	wait $C
	wait $D
	wait $E
	wait $F


	ffplay  "$JPG/%04d.jpg"
else
	read -p "skip play real: " MMM
	if [[ -z $MMM ]] || [[ "$MMM" == "n" ]]; then
		ffplay  "$JPG/%04d.jpg"
	fi
fi

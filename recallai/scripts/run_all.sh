#!/bin/bash


mkdir output-test
mkdir output

cargo build --release
cargo build --release --bin extract-images

read -p "skip expected test: " MMM
DIR=input-test
VID=$DIR/expected.rvid
JPG="$DIR/expected-jpgs"
if [[ -z $MMM ]] || [[ "$MMM" == "n" ]]; then
	echo "extracting $JPG"
	./target/release/extract-images -i $VID -o $JPG --frame-min 0 --max-width 1024
	ffplay  "$JPG/%04d.jpg"
else
	read -p "skip play expected test: " MMM
	if [[ -z $MMM ]] || [[ "$MMM" == "n" ]]; then
		ffplay  "$JPG/%04d.jpg"
	fi 
fi

read -p "skip input test: " MMM
DIR=input-test
VID=$DIR/input.rvid
JPG="$DIR/input-jpgs"
if [[ -z $MMM ]] || [[ "$MMM" == "n" ]]; then
	echo "extracting $JPG"
	./target/release/extract-images -i $VID -o $JPG --frame-min 0 --max-width 1024
	ffplay  "$JPG/%04d.jpg"
else
	read -p "skip play input test: " MMM
	if [[ -z $MMM ]] || [[ "$MMM" == "n" ]]; then
		ffplay  "$JPG/%04d.jpg"
	fi 
fi



read -p "skip xcode/extract test: " MMM
DIR=output-test
VID=$DIR/output-small.rvid
JPG="$DIR/output-small-jpgs"
if [[ -z $MMM ]] || [[ "$MMM" == "n" ]]; then
	DIR=output-test
	VID=$DIR/output-small.rvid
	JPG="$DIR/output-small-jpgs"
	./target/release/recallai-vid-transcode -i input-test/input.rvid -r input-test/rules.json -o $VID 
	echo "extracting $JPG"
	./target/release/extract-images -i $VID -o $JPG --frame-min 0 --max-width 1024
	ffplay  "$JPG/%04d.jpg"
else
	read -p "skip play test: " MMM
	if [[ -z $MMM ]] || [[ "$MMM" == "n" ]]; then
		ffplay  "$JPG/%04d.jpg"
	fi
fi

read -p "skip extract orig: " MMM
DIR=input
VID=$DIR/video.rvid
JPG="$DIR/video-jpgs"
if [[ -z $MMM ]] || [[ "$MMM" == "n" ]]; then
	echo "extracting $JPG"
	./target/release/extract-images -i $VID -o $JPG --frame-min 0 --frame-max 100 --max-width 1024
	ffplay  "$JPG/%04d.jpg"
else
	read -p "skip play orig: " MMM
	if [[ -z $MMM ]] || [[ "$MMM" == "n" ]]; then
		ffplay  "$JPG/%04d.jpg"
	fi
fi



read -p "skip xcode/extract real: " MMM
DIR=output
VID=$DIR/output.rvid
JPG="$DIR/output-jpgs"
if [[ -z $MMM ]] || [[ "$MMM" == "n" ]]; then
	./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json -o $VID
	echo "extracting $JPG"
	./target/release/extract-images -i $VID -o $JPG --frame-min 0 --frame-max 1000 --max-width 1024
	ffplay  "$JPG/%04d.jpg"
else
	read -p "skip play real: " MMM
	if [[ -z $MMM ]] || [[ "$MMM" == "n" ]]; then
		ffplay  "$JPG/%04d.jpg"
	fi
fi

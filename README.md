# WavPlayer
This repo contains a rust program for playing a wav file of an SD card using the SDIO and I2S peripherals on an STM32F4.
DMA is used for transferring PCM data to the I2S DAC while keeping the CPU free.
All modules including the exFAT, RIFF, and WAV modules were implemented from scratch.

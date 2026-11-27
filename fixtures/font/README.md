# Font fixtures

`regular.ttf` and `bold.ttf` are Lato Regular and Lato Bold from Google Fonts:

- https://github.com/google/fonts/raw/main/ofl/lato/Lato-Regular.ttf
- https://github.com/google/fonts/raw/main/ofl/lato/Lato-Bold.ttf

`regular.woff`/`bold.woff` and `regular.woff2`/`bold.woff2` are converted from
the TTF files with fontTools:

```text
python -c "from fontTools.ttLib import TTFont; f=TTFont('regular.ttf'); f.flavor='woff'; f.save('regular.woff')"
python -c "from fontTools.ttLib import TTFont; f=TTFont('regular.ttf'); f.flavor='woff2'; f.save('regular.woff2')"
```

Lato is licensed under the SIL Open Font License 1.1.

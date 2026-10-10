TARGET = harbour-regress
CONFIG += sailfishapp link_pkgconfig
PKGCONFIG += sailfishapp mlite5 \
    glib-2.0
SOURCES += src/main.cpp
# CONFIG += sailfishapp_qml
CONFIG += sailfishapp_i18n

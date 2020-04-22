/*
 Copyright 2015-2016 Lucas Augusto Deters

 This program is free software; you can redistribute it and/or modify
 it under the terms of the GNU General Public License as published by
 the Free Software Foundation; either version 2, or (at your option)
 any later version.

 This program is distributed in the hope that it will be useful,
 but WITHOUT ANY WARRANTY; without even the implied warranty of
 MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 GNU General Public License for more details.

 one line to give the program's name and an idea of what it does.
  */

#ifndef MYGESTURES_configurationPARSER_H_
#define MYGESTURES_configurationPARSER_H_

#include "configuration.h"

char *configuration_get_default_filename();

Context *configuration_load_from_file(char *filename);
Context *configuration_load_from_defaults();

#endif

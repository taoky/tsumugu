package main

import "errors"

// ErrEnterpointHasNoTrailingSlash Enterpoint URL shall have trailing slash (like https://example.com/test/)
var ErrEnterpointHasNoTrailingSlash = errors.New("no trailing slash found")

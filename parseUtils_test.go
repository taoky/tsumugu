package main

import (
	"net/url"
	"testing"
)

func baseURLTest(base string, href string, expected string, t *testing.T) {
	url, err := url.Parse(base)
	if err != nil {
		t.Fatal(err)
	}
	actual, err := urlBuilder(url, href)
	if err != nil {
		t.Fatal(err)
	}
	if actual.String() != expected {
		t.Errorf("Expected %s, got %s. base = %s, href = %s\n", expected, actual.String(), base, href)
	}
}

// func baseEnterpointValidationTest(base string, expected error, t *testing.T) {
// 	url, err := url.Parse(base)
// 	err = validateEnterpoint(url)
// 	if err != expected {
// 		t.Errorf("Expected %s, got %s. base = %s\n", expected, err, base)
// 	}
// }

func TestUrlBuilder(t *testing.T) {
	baseURLTest("https://download.docker.com", "linux", "https://download.docker.com/linux", t)
	baseURLTest("https://download.docker.com/linux/", "centos/", "https://download.docker.com/linux/centos/", t)
}

// func TestEnterpointValidation(t *testing.T) {
// 	baseEnterpointValidationTest("https://download.docker.com/linux", ErrEnterpointHasNoTrailingSlash, t)
// }
